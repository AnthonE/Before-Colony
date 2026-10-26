//! Reading a shader's numbers in tests, to check them against the rules they paint.

/// The value of `const NAME: … = v;` in a WGSL source (a float, or an unsigned integer `1u`).
pub fn constant(src: &str, name: &str) -> f32 {
    let line = src
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("const {name}:")))
        .unwrap_or_else(|| panic!("no const {name}"));
    let v = line.split('=').nth(1).unwrap().trim().trim_end_matches(';').trim().trim_end_matches('u');
    v.parse().unwrap_or_else(|_| panic!("{name} = {v}"))
}
