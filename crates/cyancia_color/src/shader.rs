use anyhow::{Result, bail};
use moxcms::{
    CmsError, ColorProfile, DataColorSpace, Layout, Matrix3d, ParametricCurve, ToneReprCurve,
    TransformOptions,
};

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
