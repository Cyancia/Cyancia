import package::rgb::Rgb;

struct Hsv {
    h: f32,
    s: f32,
    v: f32,
}

fn hsv_from_rgb(rgb: Rgb) -> Hsv {
    let max_value = max(rgb.r, max(rgb.g, rgb.b));
    let min_value = min(rgb.r, min(rgb.g, rgb.b));
    let delta = max_value - min_value;

    if delta == 0.0 {
        return Hsv(0.0, 0.0, max_value);
    }

    var hue_sector = 0.0;
    if max_value == rgb.r {
        hue_sector = (rgb.g - rgb.b) / delta;
    } else if max_value == rgb.g {
        hue_sector = (rgb.b - rgb.r) / delta + 2.0;
    } else {
        hue_sector = (rgb.r - rgb.g) / delta + 4.0;
    }

    let hue = ((hue_sector * 60.0) % 360.0 + 360.0) % 360.0;
    return Hsv(hue, delta / max_value, max_value);
}

fn hsv_into_rgb(hsv: Hsv) -> Rgb {
    let chroma = hsv.v * hsv.s;
    let hue_sector = (((hsv.h % 360.0) + 360.0) % 360.0) / 60.0;
    let x = chroma * (1.0 - abs((hue_sector % 2.0) - 1.0));
    var rgb = vec3f(0.0);

    if hue_sector < 1.0 {
        rgb = vec3f(chroma, x, 0.0);
    } else if hue_sector < 2.0 {
        rgb = vec3f(x, chroma, 0.0);
    } else if hue_sector < 3.0 {
        rgb = vec3f(0.0, chroma, x);
    } else if hue_sector < 4.0 {
        rgb = vec3f(0.0, x, chroma);
    } else if hue_sector < 5.0 {
        rgb = vec3f(x, 0.0, chroma);
    } else {
        rgb = vec3f(chroma, 0.0, x);
    }

    let m = hsv.v - chroma;
    return Rgb(rgb.r + m, rgb.g + m, rgb.b + m);
}
