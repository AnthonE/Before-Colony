// Hash and value noise shared by the procedural materials.
#define_import_path bc::noise

fn hash13(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.1031);
    q += dot(q, q.zyx + 31.32);
    return fract((q.x + q.y) * q.z);
}

fn hash23(p: vec3<f32>) -> vec2<f32> {
    var q = fract(p * vec3<f32>(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yzx + 33.33);
    return fract((q.xx + q.yz) * q.zy);
}

// Smooth value noise in [0, 1].
fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(hash13(i), hash13(i + vec3(1.0, 0.0, 0.0)), u.x);
    let b = mix(hash13(i + vec3(0.0, 1.0, 0.0)), hash13(i + vec3(1.0, 1.0, 0.0)), u.x);
    let c = mix(hash13(i + vec3(0.0, 0.0, 1.0)), hash13(i + vec3(1.0, 0.0, 1.0)), u.x);
    let d = mix(hash13(i + vec3(0.0, 1.0, 1.0)), hash13(i + vec3(1.0, 1.0, 1.0)), u.x);
    return mix(mix(a, b, u.y), mix(c, d, u.y), u.z);
}

// Fractal value noise, about [0, 1].
fn fbm(p: vec3<f32>, octaves: i32) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var norm = 0.0;
    var q = p;
    for (var i = 0; i < octaves; i++) {
        sum += amp * noise3(q);
        norm += amp;
        q = q * 2.03 + vec3(1.7, 9.2, 3.1);
        amp *= 0.5;
    }
    return sum / norm;
}
