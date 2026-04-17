const TILE_SIZE: u32 = 256;
const MAX_TILE_COUNT: u32 = 2048;

struct TileInfo {
    index: vec2i,
    origin: vec2i,
}

struct DynamicTileInfo {
    n_tiles: atomic<u32>,
    buf: array<TileInfo>,
}
