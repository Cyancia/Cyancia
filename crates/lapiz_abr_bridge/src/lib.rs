// DISCLAIMER
//
// This crate was developed exclusively through manual clean room reverse
// engineering, and the following statements are made with respect to that
// work:
//
// 1. The implementation relies solely on publicly available documentation of
//    the Adobe Brush (ABR) file format and on other existing, independently
//    developed implementations of that format.
//
// 2. Adobe Photoshop was used only as a reference for artifacts produced by
//    hand: sample ABR files were created manually through the Photoshop user
//    interface and examined as reference material for this crate.
//
// 3. No Adobe Photoshop binary, library, or other executable component was
//    disassembled, decompiled, or otherwise inspected.
//
// 4. No script, tool, or automated process was used to run, probe, instrument,
//    or debug Adobe Photoshop.
//
// This crate is an independent implementation of the ABR format. It contains
// no Adobe software and is not affiliated with, endorsed by, or sponsored by
// Adobe Inc.

use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use lapiz_abr::Abr;
use lapiz_assets::{
    asset::{AssetId, ErasedAsset, UntypedAssetId},
    bundle::{AssetBundle, AssetBundleMetadata, BundleId, BundleManifest},
    loader::ErasedAssetSerializer,
    tag::{AssetTags, TagFile},
};
use lapiz_render::texture::Image;
use thiserror::Error;
use uuid::Uuid;
use xxhash_rust::xxh3::xxh3_128;

pub mod desc;
pub mod patt;
pub mod samp;

pub struct AbrAssetBundle {
    path: PathBuf,
    metadata: AssetBundleMetadata,
    manifest: BundleManifest,
    assets: HashMap<PathBuf, Arc<dyn ErasedAsset>>,
}

#[derive(Debug, Error)]
pub enum AbrAssetBundleError {
    #[error("Unsupported writing to ABR asset bundle")]
    UnsupportedWriting,
    #[error("Asset not found at path: {0}")]
    AssetNotFound(PathBuf),
    #[error("Tag not found at path: {0}")]
    TagNotFound(PathBuf),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("ABR parse error: {0}")]
    AbrParse(#[from] anyhow::Error),
}

impl AbrAssetBundle {
    pub fn parse(path: impl AsRef<Path>, abr: Abr) -> Self {
        let path = path.as_ref().to_path_buf();
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bundle_id = BundleId::new(Uuid::from_u128(xxh3_128(name.as_bytes())));
        let last_modified = std::fs::metadata(&path)
            .and_then(|metadata| metadata.modified())
            .map(DateTime::<Utc>::from)
            .unwrap_or_else(|_| Utc::now());

        let metadata = AssetBundleMetadata {
            bundle_id,
            name,
            last_modified,
        };
        let mut manifest = BundleManifest {
            assets: BTreeMap::new(),
            tags: BTreeMap::new(),
        };
        let mut assets = HashMap::new();
        let mut sample_assets = HashMap::<Uuid, AssetId<Image>>::new();
        let mut pattern_assets = HashMap::<Uuid, AssetId<Image>>::new();

        for sample in &abr.samples {
            let asset_id = UntypedAssetId::new(Uuid::new_v5(&bundle_id.0, sample.id.as_bytes()));
            let path = PathBuf::from(format!("samp-{asset_id}.lig"));
            match samp::parse_samp(sample) {
                Ok(asset) => {
                    sample_assets.insert(sample.id, asset_id.into_typed());
                    manifest.assets.insert(asset_id, path.clone());
                    assets.insert(path, Arc::new(asset) as Arc<dyn ErasedAsset>);
                }
                Err(error) => {
                    log::error!("Failed to convert samp {}: {error}", sample.id);
                }
            }
        }

        for pattern in &abr.patterns {
            let asset_id = UntypedAssetId::new(Uuid::new_v5(&bundle_id.0, pattern.id.as_bytes()));
            let path = PathBuf::from(format!("patt-{asset_id}.lig"));
            match patt::parse_patt(pattern) {
                Ok(asset) => {
                    pattern_assets.insert(pattern.id, asset_id.into_typed());
                    manifest.assets.insert(asset_id, path.clone());
                    assets.insert(path, Arc::new(asset) as Arc<dyn ErasedAsset>);
                }
                Err(error) => {
                    log::error!(
                        "Failed to convert patt {} ({}): {error}",
                        pattern.name,
                        pattern.id
                    );
                }
            }
        }

        for brush in &abr.descriptors {
            let asset_id = UntypedAssetId::new(Uuid::new_v5(&bundle_id.0, brush.name.as_bytes()));
            let path = PathBuf::from(format!("desc-{asset_id}.lapiz"));
            match desc::parse_desc(brush, &sample_assets, &pattern_assets) {
                Ok(asset) => {
                    manifest.assets.insert(asset_id, path.clone());
                    assets.insert(path, Arc::new(asset) as Arc<dyn ErasedAsset>);
                }
                Err(error) => {
                    log::error!("Failed to convert desc {}: {error}", brush.name);
                }
            }
        }

        Self {
            path,
            metadata,
            manifest,
            assets,
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, AbrAssetBundleError> {
        let path = path.as_ref().to_path_buf();
        let abr = Abr::parse(&std::fs::read(&path)?)?;
        Ok(Self::parse(path, abr))
    }

    pub fn scan_bundles(root: impl AsRef<Path>) -> (Vec<Self>, Vec<AbrAssetBundleError>) {
        let mut bundles = Vec::new();
        let mut errors = Vec::new();
        scan_bundles_dfs(root.as_ref(), &mut bundles, &mut errors);
        (bundles, errors)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn scan_bundles_dfs(
    root: &Path,
    bundles: &mut Vec<AbrAssetBundle>,
    errors: &mut Vec<AbrAssetBundleError>,
) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(error.into());
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(error.into());
                continue;
            }
        };
        let path = entry.path();
        if path.is_file() {
            if path.extension() == Some(OsStr::new("abr")) {
                match AbrAssetBundle::open(&path) {
                    Ok(bundle) => bundles.push(bundle),
                    Err(error) => errors.push(error),
                }
            }
        } else if path.is_dir() {
            scan_bundles_dfs(&path, bundles, errors);
        }
    }
}

impl AssetBundle for AbrAssetBundle {
    const READONLY: bool = true;

    type Error = AbrAssetBundleError;

    fn metadata(&self) -> Result<AssetBundleMetadata, Self::Error> {
        Ok(self.metadata.clone())
    }

    fn manifest(&self) -> Result<BundleManifest, Self::Error> {
        Ok(self.manifest.clone())
    }

    fn read_asset(
        &self,
        path: &Path,
        _: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Self::Error> {
        self.assets
            .get(path)
            .cloned()
            .ok_or_else(|| AbrAssetBundleError::AssetNotFound(path.to_path_buf()))
    }

    fn add_asset(
        &self,
        _: &Path,
        _: &dyn ErasedAsset,
        _: &dyn ErasedAssetSerializer,
    ) -> Result<UntypedAssetId, Self::Error> {
        Err(AbrAssetBundleError::UnsupportedWriting)
    }

    fn read_tag(&self, tag: &Path) -> Result<TagFile, Self::Error> {
        Err(AbrAssetBundleError::TagNotFound(tag.to_path_buf()))
    }

    fn add_tag(&self, _: &Path, _: &TagFile) -> Result<(), Self::Error> {
        Err(AbrAssetBundleError::UnsupportedWriting)
    }

    fn read_asset_tags(&self, _: &Path) -> Result<Option<AssetTags>, Self::Error> {
        Ok(None)
    }

    fn write_asset_tags(&self, _: &Path, _: &AssetTags) -> Result<(), Self::Error> {
        Err(AbrAssetBundleError::UnsupportedWriting)
    }
}
