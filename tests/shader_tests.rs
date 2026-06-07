//! Validate the WGSL shaders parse and pass naga validation, so a shader typo fails
//! `cargo test` instead of only failing in the browser at runtime.

fn validate(src: &str, name: &str) {
    let module = naga::front::wgsl::parse_str(src)
        .unwrap_or_else(|e| panic!("{name} failed to parse:\n{e:?}"));
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    validator
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name} failed naga validation:\n{e:?}"));
}

#[test]
fn shaders_parse_and_validate() {
    validate(include_str!("../shaders/waves.wgsl"), "waves.wgsl");
    validate(include_str!("../shaders/post.wgsl"), "post.wgsl");
}
