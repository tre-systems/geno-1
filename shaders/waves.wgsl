// Bundled into the WASM binary via include_str! in src/core/mod.rs.
// Audio-reactive ribbon/heightfield aesthetic rendered in a single fullscreen pass.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Voice {
    // xyz position (x,z used), w = pulse (0..1.5)
    pos_pulse: vec4<f32>,
};

struct WaveUniforms {
    resolution: vec2<f32>,
    time: f32,
    ambient: f32,
    voices: array<Voice, 3>,
    swirl_uv: vec2<f32>,
    swirl_strength: f32,
    swirl_active: f32,
    ripple_uv: vec2<f32>,
    ripple_t0: f32,
    ripple_amp: f32,
};

@group(0) @binding(0) var<uniform> u: WaveUniforms;

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

fn palette(t: f32) -> vec3<f32> {
    let a = vec3<f32>(0.46, 0.52, 0.62);
    let b = vec3<f32>(0.40, 0.32, 0.20);
    let c = vec3<f32>(0.88, 0.74, 0.58);
    let d = vec3<f32>(0.22, 0.14, 0.05);
    return a + b * cos(6.28318 * (c * t + d));
}

@fragment
fn fs_waves(inp: VsOut) -> @location(0) vec4<f32> {
    let uv = inp.uv;
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    let cuv0 = (uv - 0.5) * vec2<f32>(aspect, 1.0);
    let t = u.time;

    let warm = vec3<f32>(1.04, 0.76, 0.36);
    let moon = vec3<f32>(0.62, 0.85, 1.06);
    var col = vec3<f32>(0.010, 0.015, 0.034);

    // Layered wave sheets with depth parallax.
    for (var L = 0; L < 3; L = L + 1) {
        let depth = f32(L);
        let d01 = depth / 2.0;
        let par = mix(0.62, 1.35, d01);
        var cuv = cuv0 * par + vec2<f32>(0.0, -0.12 * depth);

        // Slow layer drift adds an organic camera feel.
        cuv += vec2<f32>(
            0.09 * sin(0.17 * t + depth * 1.2),
            0.07 * cos(0.14 * t - depth * 1.6),
        );

        // Pointer-driven swirl distortion.
        let swirl_center = (u.swirl_uv - 0.5) * vec2<f32>(aspect, 1.0) * par;
        let v = cuv - swirl_center;
        let r = length(v);
        let ang = u.swirl_active * u.swirl_strength * 2.9 * exp(-1.9 * r);
        let cs = cos(ang);
        let sn = sin(ang);
        cuv = swirl_center + vec2<f32>(v.x * cs - v.y * sn, v.x * sn + v.y * cs);

        // Voice displacement field.
        var disp = vec2<f32>(0.0);
        for (var i = 0; i < 3; i = i + 1) {
            let voice = u.voices[i];
            let p = vec2<f32>(voice.pos_pulse.x, voice.pos_pulse.z) * 0.33;
            let d = distance(cuv, p);
            let dir = (cuv - p) / max(d, 1e-4);
            let pulse = clamp(voice.pos_pulse.w, 0.0, 1.5);
            let str = (0.10 + 0.46 * pulse) * exp(-1.9 * d);
            disp += dir * str;
        }
        cuv += disp;

        // Heightfield synthesis.
        let tt = t * (0.24 + 0.09 * depth);
        let amp = mix(0.92, 2.2, d01);
        var h = 0.0;
        h += amp * (0.92 * sin((4.8 + 1.1 * depth) * cuv.x - 1.35 * tt));
        h += amp * (0.72 * cos((6.6 + 1.3 * depth) * cuv.y + 0.90 * tt));
        h += amp * (0.33 * sin((9.6 + 1.7 * depth) * (cuv.x + 0.45 * cuv.y) - 0.66 * tt));
        h += 0.30 * fbm(cuv * (2.1 + 0.35 * depth) + vec2<f32>(0.24 * tt, -0.18 * tt));
        h *= (1.0 - 0.22 * abs(cuv.y));

        for (var i = 0; i < 3; i = i + 1) {
            let voice = u.voices[i];
            let p = vec2<f32>(voice.pos_pulse.x, voice.pos_pulse.z) * 0.33;
            let d = distance(cuv, p);
            let pulse = clamp(voice.pos_pulse.w, 0.0, 1.5);
            h += (0.52 + 0.82 * pulse) * exp(-2.2 * d) * sin(12.0 * d - (1.8 + 0.2 * depth) * tt);
            h += (0.16 + 0.24 * pulse) * exp(-8.2 * d * d) * sin(7.4 * (cuv.x - p.x) + 1.6 * tt);
        }

        // Click ripple.
        let ripple_center = (u.ripple_uv - 0.5) * vec2<f32>(aspect, 1.0) * par;
        let rv = cuv - ripple_center;
        let rr = length(rv);
        let age = max(0.0, t - u.ripple_t0);
        let ripple_env = u.ripple_amp * exp(-2.2 * age) * exp(-3.8 * rr);
        h += ripple_env * sin(mix(15.0, 20.0, d01) * rr - 7.0 * age);

        // Derivative-based normal for coherent shading.
        let n = normalize(vec3<f32>(-1.7 * dpdx(h), -1.7 * dpdy(h), 1.0));
        let l1 = normalize(vec3<f32>(-0.34, 0.44, 0.83));
        let l2 = normalize(vec3<f32>(0.60, -0.16, 0.78));
        let diff = 0.66 * max(dot(n, l1), 0.0) + 0.34 * max(dot(n, l2), 0.0);

        let hue = 0.22 * depth + 0.17 * h + 0.06 * fbm(cuv * 1.6 + vec2<f32>(0.11 * tt, -0.08 * tt));
        let spectral = palette(hue);
        let base = mix(vec3<f32>(0.018, 0.026, 0.056), vec3<f32>(0.10, 0.14, 0.25), 0.30 + 0.52 * diff + 0.18 * u.ambient);
        let k = clamp(0.5 + 0.78 * h, 0.0, 1.0);

        var layer_col = base + spectral * (0.30 + 0.34 * u.ambient);
        layer_col += mix(moon * 0.18, warm * 0.24, k);

        let ribbons = smoothstep(0.47, 0.50, abs(fract(h * 5.8 + depth * 0.14) - 0.5));
        layer_col += (1.0 - ribbons) * warm * (0.12 + 0.18 * u.ambient);

        let view = vec3<f32>(0.0, 0.0, 1.0);
        let h1 = normalize(l1 + view);
        let spec = pow(max(dot(n, h1), 0.0), 64.0);
        layer_col += vec3<f32>(1.0, 0.97, 0.90) * (0.16 * spec);

        let crest = smoothstep(0.72, 0.98, k);
        layer_col += warm * crest * (0.34 + 0.84 * u.ambient);

        for (var i = 0; i < 3; i = i + 1) {
            let voice = u.voices[i];
            let p = vec2<f32>(voice.pos_pulse.x, voice.pos_pulse.z) * 0.33;
            let d = distance(cuv, p);
            let pulse = clamp(voice.pos_pulse.w, 0.0, 1.5);
            layer_col += (warm + moon * 0.45) * exp(-36.0 * d * d) * (0.14 + 0.22 * pulse);
        }

        let ring = smoothstep(0.010, 0.002, abs(rr - (0.19 * age + 0.02)));
        layer_col += warm * clamp(u.ripple_amp * exp(-1.3 * age) * ring, 0.0, 1.0) * 0.58;

        let a = mix(0.58, 0.30, d01);
        col = col * (1.0 - a) + layer_col * a;
    }

    // Atmospheric finishing pass.
    let vignette = 1.0 - smoothstep(0.38, 1.12, length(cuv0));
    let halo = exp(-3.9 * length(cuv0));
    col *= mix(0.74, 1.06, vignette);
    col += vec3<f32>(0.022, 0.040, 0.090) * halo * (0.28 + 0.72 * u.ambient);

    let grain = hash2(cuv0 * 640.0 + vec2<f32>(0.13 * t, -0.09 * t));
    col += (grain - 0.5) * 0.012;

    return vec4<f32>(col, 1.0);
}
