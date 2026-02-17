// Copy exists under app-web for bundling via core module include_str!
// Fullscreen post-processing: HDR bright pass, separable blur, and graded composite.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct PostUniforms {
    resolution: vec2<f32>,
    time: f32,
    ambient: f32,
    blur_dir: vec2<f32>,
    bloom_strength: f32,
    threshold: f32,
}

@group(0) @binding(0) var hdr_tex: texture_2d<f32>;
@group(0) @binding(1) var hdr_sampler: sampler;
@group(0) @binding(2) var<uniform> u_post: PostUniforms;

@group(1) @binding(0) var blur_tex: texture_2d<f32>;
@group(1) @binding(1) var blur_sampler: sampler;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vid: u32) -> VsOut {
    let pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(3.0, 1.0),
    );
    let uv = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 2.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
    );

    var out: VsOut;
    out.pos = vec4<f32>(pos[vid], 0.0, 1.0);
    out.uv = uv[vid];
    return out;
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn vignette_mask(uv: vec2<f32>) -> f32 {
    let p = (uv - 0.5) * vec2<f32>(1.15, 1.0);
    let r = length(p);
    return 1.0 - smoothstep(0.30, 0.92, r);
}

fn hash2(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn fbm(p: vec2<f32>) -> f32 {
    var a = 0.0;
    var b = 0.5;
    var f = p;
    for (var i = 0; i < 5; i = i + 1) {
        a += b * sin(f.x) * cos(f.y);
        f *= 2.17;
        b *= 0.55;
    }
    return a;
}

// BRIGHT PASS: extract highlights above threshold for bloom.
@fragment
fn fs_bright(inp: VsOut) -> @location(0) vec4<f32> {
    let col = textureSample(hdr_tex, hdr_sampler, inp.uv).rgb;
    let thr = u_post.threshold;
    let l = luminance(col);
    let k = max(l - thr, 0.0);
    let outc = col * (k / max(l, 1e-5));
    return vec4<f32>(outc, 1.0);
}

// BLUR PASS: 7-tap Gaussian blur along specified direction.
@fragment
fn fs_blur(inp: VsOut) -> @location(0) vec4<f32> {
    let texel = u_post.blur_dir / u_post.resolution;

    let w0 = 0.05;
    let w1 = 0.09;
    let w2 = 0.12;
    let w3 = 0.15;

    var acc = vec3<f32>(0.0);
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv - texel * 3.0).rgb * w0;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv - texel * 2.0).rgb * w1;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv - texel * 1.0).rgb * w2;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv).rgb * w3;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv + texel * 1.0).rgb * w2;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv + texel * 2.0).rgb * w1;
    acc += textureSample(hdr_tex, hdr_sampler, inp.uv + texel * 3.0).rgb * w0;

    return vec4<f32>(acc, 1.0);
}

// COMPOSITE: final composition with grading, tonemapping, and atmosphere.
@fragment
fn fs_composite(inp: VsOut) -> @location(0) vec4<f32> {
    let center = inp.uv - 0.5;
    let edge = smoothstep(0.22, 0.85, length(center) * 1.4142);
    let dir = normalize(center + vec2<f32>(1e-5, 0.0));
    let ca = (0.0012 + 0.0018 * u_post.ambient) * edge;

    // Slight chromatic separation near the frame edges.
    let r = textureSample(hdr_tex, hdr_sampler, inp.uv + dir * ca).r;
    let g = textureSample(hdr_tex, hdr_sampler, inp.uv).g;
    let b = textureSample(hdr_tex, hdr_sampler, inp.uv - dir * ca).b;
    var base = vec3<f32>(r, g, b);

    let bloom = textureSample(blur_tex, blur_sampler, inp.uv).rgb * u_post.bloom_strength;
    base += bloom;

    let t = u_post.time * 0.11;
    let tint = vec3<f32>(
        1.0 + 0.07 * sin(t + 0.3),
        1.0 + 0.05 * sin(t * 1.3 + 2.2),
        1.0 + 0.08 * sin(t * 0.9 + 4.0)
    );
    base *= mix(vec3<f32>(1.0), tint, 0.12 + 0.28 * u_post.ambient);

    // Exposure before tonemap.
    base *= 0.86;

    var mapped = aces_tonemap(base);

    // Contrast and gentle channel-aware gamma.
    mapped = clamp((mapped - vec3<f32>(0.5)) * 1.12 + vec3<f32>(0.5), vec3<f32>(0.0), vec3<f32>(1.0));
    mapped = pow(mapped, vec3<f32>(1.03, 1.01, 0.99));

    // Split-toned color grade.
    let luma = luminance(mapped);
    let grade = mix(
        vec3<f32>(0.88, 0.94, 1.06),
        vec3<f32>(1.08, 0.98, 0.90),
        smoothstep(0.24, 0.86, luma)
    );
    mapped *= grade;

    let uv = inp.uv;
    let smoke_a = 0.5 + 0.5 * fbm(uv * 2.7 + vec2<f32>(0.05 * u_post.time, -0.04 * u_post.time));
    let smoke_b = 0.5 + 0.5 * fbm((uv.yx + vec2<f32>(0.13, -0.08)) * 3.2 + vec2<f32>(-0.03 * u_post.time, 0.05 * u_post.time));
    let smoke = clamp(0.5 * smoke_a + 0.5 * smoke_b, 0.0, 1.0);
    let radial = smoothstep(0.24, 0.98, length(center) * 1.35);
    let smoke_k = 0.16 * smoke * radial;
    mapped = mapped * (1.0 - smoke_k) + vec3<f32>(0.03, 0.05, 0.09) * (smoke_k * 0.32);

    let vig = vignette_mask(inp.uv);
    mapped *= mix(0.58, 1.0, vig);

    // Fine film grain and subtle scanline shimmer.
    let noise = hash2(inp.uv * u_post.resolution + vec2<f32>(31.0 * t, 19.0 * t));
    mapped += (noise - 0.5) * 0.018;

    let scan = sin((inp.uv.y * u_post.resolution.y + u_post.time * 14.0) * 0.5);
    mapped *= 1.0 + 0.008 * scan;

    return vec4<f32>(clamp(mapped, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
