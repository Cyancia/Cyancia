use anyhow::{Result, bail};
use moxcms::{
    CmsError, ColorProfile, DataColorSpace, Layout, Matrix3d, ParametricCurve, ToneReprCurve,
    TransformOptions,
};

pub struct IccInputTransformShader {
    pub function: String,
}

impl IccInputTransformShader {
    pub fn new(ident: &str, profile: &ColorProfile, _: Layout) -> Result<Self> {
        if !profile.is_matrix_shaper() {
            bail!("Only matrix shaper profiles are supported yet.");
        }

        if profile.color_space != DataColorSpace::Rgb {
            bail!("Only RGB color spaces are supported yet.");
        }

        let r_trc = profile.red_trc.as_ref().unwrap();
        let g_trc = profile.green_trc.as_ref().unwrap();
        let b_trc = profile.blue_trc.as_ref().unwrap();

        let r_trc_inv_fn_ident = format!("_{}_r_trc_inv", ident);
        let g_trc_inv_fn_ident = format!("_{}_g_trc_inv", ident);
        let b_trc_inv_fn_ident = format!("_{}_b_trc_inv", ident);

        let r_trc_inv_fn = linear_function(&r_trc_inv_fn_ident, "f32", r_trc)?;
        let g_trc_inv_fn = linear_function(&g_trc_inv_fn_ident, "f32", g_trc)?;
        let b_trc_inv_fn = linear_function(&b_trc_inv_fn_ident, "f32", b_trc)?;

        let rgb_to_xyz_fn_ident = format!("_{}_rgb_to_xyz", ident);
        let rgb_to_xyz_body = matrix_function(&rgb_to_xyz_fn_ident, &profile.rgb_to_xyz_matrix())?;

        let function = format!(
            "
            fn {ident}(c: vec3f) -> vec3f {{
                let _r = {r_trc_inv_fn_ident}(c[0]);
                let _g = {g_trc_inv_fn_ident}(c[1]);
                let _b = {b_trc_inv_fn_ident}(c[2]);

                return {rgb_to_xyz_fn_ident}(vec3f(_r, _g, _b));
            }}

            {r_trc_inv_fn}
            {g_trc_inv_fn}
            {b_trc_inv_fn}
            {rgb_to_xyz_body}
        "
        );

        Ok(Self { function })
    }
}

pub struct IccOutputTransformShader {
    pub function: String,
}

impl IccOutputTransformShader {
    pub fn new(ident: &str, profile: &ColorProfile, _layout: Layout) -> Result<Self> {
        if !profile.is_matrix_shaper() {
            bail!("Only matrix shaper profiles are supported yet.");
        }

        if profile.color_space != DataColorSpace::Rgb {
            bail!("Only RGB color spaces are supported yet.");
        }

        let r_trc = profile.red_trc.as_ref().unwrap();
        let g_trc = profile.green_trc.as_ref().unwrap();
        let b_trc = profile.blue_trc.as_ref().unwrap();

        let r_trc_fn_ident = format!("_{}_r_trc", ident);
        let g_trc_fn_ident = format!("_{}_g_trc", ident);
        let b_trc_fn_ident = format!("_{}_b_trc", ident);

        let r_trc_fn = gamma_function(&r_trc_fn_ident, "f32", r_trc)?;
        let g_trc_fn = gamma_function(&g_trc_fn_ident, "f32", g_trc)?;
        let b_trc_fn = gamma_function(&b_trc_fn_ident, "f32", b_trc)?;

        let xyz_to_rgb_fn_ident = format!("_{}_xyz_to_rgb", ident);
        let xyz_to_rgb_fn =
            matrix_function(&xyz_to_rgb_fn_ident, &profile.rgb_to_xyz_matrix().inverse())?;

        let function = format!(
            "
            fn {ident}(c: vec3f) -> vec3f {{
                let _xyz = {xyz_to_rgb_fn_ident}(c);
                let _r = {r_trc_fn_ident}(_xyz[0]);
                let _g = {g_trc_fn_ident}(_xyz[1]);
                let _b = {b_trc_fn_ident}(_xyz[2]);

                return vec3f(_r, _g, _b);
            }}

            {r_trc_fn}
            {g_trc_fn}
            {b_trc_fn}
            {xyz_to_rgb_fn}
        "
        );

        Ok(Self { function })
    }
}

pub struct IccTransformShader {
    pub function: String,
}

impl IccTransformShader {
    pub fn new(
        ident: &str,
        src_pr: &ColorProfile,
        _: Layout,
        dst_pr: &ColorProfile,
        _: Layout,
        _: TransformOptions,
    ) -> Result<Self> {
        if !src_pr.is_matrix_shaper() || !dst_pr.is_matrix_shaper() {
            bail!("Only matrix shaper profiles are supported yet.");
        }

        if src_pr.color_space != DataColorSpace::Rgb || dst_pr.color_space != DataColorSpace::Rgb {
            bail!("Only RGB color spaces are supported yet.");
        }

        let src_r_trc = src_pr.red_trc.as_ref().unwrap();
        let src_g_trc = src_pr.green_trc.as_ref().unwrap();
        let src_b_trc = src_pr.blue_trc.as_ref().unwrap();

        let src_r_trc_inv_fn_ident = format!("_{}_src_r_trc_inv", ident);
        let src_g_trc_inv_fn_ident = format!("_{}_src_g_trc_inv", ident);
        let src_b_trc_inv_fn_ident = format!("_{}_src_b_trc_inv", ident);

        let src_r_trc_inv_fn = linear_function(&src_r_trc_inv_fn_ident, "f32", src_r_trc)?;
        let src_g_trc_inv_fn = linear_function(&src_g_trc_inv_fn_ident, "f32", src_g_trc)?;
        let src_b_trc_inv_fn = linear_function(&src_b_trc_inv_fn_ident, "f32", src_b_trc)?;

        let dst_r_trc = dst_pr.red_trc.as_ref().unwrap();
        let dst_g_trc = dst_pr.green_trc.as_ref().unwrap();
        let dst_b_trc = dst_pr.blue_trc.as_ref().unwrap();

        let dst_r_trc_fn_ident = format!("_{}_dst_r_trc", ident);
        let dst_g_trc_fn_ident = format!("_{}_dst_g_trc", ident);
        let dst_b_trc_fn_ident = format!("_{}_dst_b_trc", ident);

        let dst_r_trc_fn = gamma_function(&dst_r_trc_fn_ident, "f32", dst_r_trc)?;
        let dst_g_trc_fn = gamma_function(&dst_g_trc_fn_ident, "f32", dst_g_trc)?;
        let dst_b_trc_fn = gamma_function(&dst_b_trc_fn_ident, "f32", dst_b_trc)?;

        let combined_matrix = src_pr.transform_matrix(dst_pr);
        let combined_matrix_fn_ident = format!("_{}_combined_matrix", ident);
        let combined_matrix_fn = matrix_function(&combined_matrix_fn_ident, &combined_matrix)?;

        let function = format!(
            "fn {ident}(c: vec3f) -> vec3f {{
                let _r = {src_r_trc_inv_fn_ident}(c[0]);
                let _g = {src_g_trc_inv_fn_ident}(c[1]);
                let _b = {src_b_trc_inv_fn_ident}(c[2]);

                let _rgb = {combined_matrix_fn_ident}(vec3f(_r, _g, _b));

                return vec3f(
                    {dst_r_trc_fn_ident}(_rgb[0]),
                    {dst_g_trc_fn_ident}(_rgb[1]),
                    {dst_b_trc_fn_ident}(_rgb[2])
                );
            }}

            {src_r_trc_inv_fn}
            {src_g_trc_inv_fn}
            {src_b_trc_inv_fn}
            {combined_matrix_fn}
            {dst_r_trc_fn}
            {dst_g_trc_fn}
            {dst_b_trc_fn}
            "
        );

        Ok(Self { function })
    }

    pub fn unmanaged(ident: &str) -> Self {
        let function = format!(
            "fn {ident}(c: vec3f) -> vec3f {{
                return c;
            }}
            "
        );

        Self { function }
    }
}

fn linear_function(ident: &str, component_ty: &str, trc: &ToneReprCurve) -> Result<String> {
    let body = match trc {
        ToneReprCurve::Lut(lut) => {
            if lut.is_empty() {
                "return x;".to_string()
            } else if lut.len() == 1 {
                let gamma = (lut[0] as i32 as f64 / 256.0) as f32;
                format!("return pow(x, {gamma});")
            } else {
                bail!("Lut trcs are not supported yet.")
            }
        }
        ToneReprCurve::Parametric(parametric) => {
            let ParametricCurve {
                g,
                a,
                b,
                c,
                d,
                e,
                f,
            } = ParametricCurve::new(parametric).ok_or(CmsError::BuildTransferFunction)?;
            format!(
                "if all(x < {component_ty}({d:e})) {{
                    return {c:e} * x + {f:e};
                }} else {{
                    return pow({a:e} * x + {b:e}, {g:e}) + {e:e};
                }}"
            )
        }
    };

    Ok(format!(
        "fn {ident}(x: {component_ty}) -> {component_ty} {{
            {body}
        }}"
    ))
}

fn gamma_function(ident: &str, component_ty: &str, trc: &ToneReprCurve) -> Result<String> {
    let body = match trc {
        ToneReprCurve::Lut(lut) => {
            if lut.is_empty() {
                "return x;".to_string()
            } else if lut.len() == 1 {
                let gamma = 1.0 / (lut[0] as i32 as f64 / 256.0) as f32;
                format!("return pow(x, {gamma});")
            } else {
                bail!("Lut trcs are not supported yet.")
            }
        }
        ToneReprCurve::Parametric(parametric) => {
            let ParametricCurve {
                g,
                a,
                b,
                c,
                d,
                e,
                f,
            } = ParametricCurve::new(parametric)
                .and_then(|c| c.invert())
                .ok_or(CmsError::BuildTransferFunction)?;
            format!(
                "if all(x < {component_ty}({d:e})) {{
                    return {c:e} * x + {f:e};
                }} else {{
                    return pow({a:e} * x + {b:e}, {g:e}) + {e:e};
                }}"
            )
        }
    };

    Ok(format!(
        "fn {ident}(x: {component_ty}) -> {component_ty} {{
            {body}
        }}"
    ))
}

fn matrix_function(ident: &str, matrix: &Matrix3d) -> Result<String> {
    let m = &matrix.v;
    Ok(format!(
        // Matrices in wgsl are column major
        "fn {ident}(x: vec3f) -> vec3f {{
            const MAT = mat3x3f(
                {:e}, {:e}, {:e},
                {:e}, {:e}, {:e},
                {:e}, {:e}, {:e}
            );
            return MAT * x;
        }}",
        // While in moxcms, they are row major
        m[0][0],
        m[1][0],
        m[2][0],
        m[0][1],
        m[1][1],
        m[2][1],
        m[0][2],
        m[1][2],
        m[2][2],
    ))
}
