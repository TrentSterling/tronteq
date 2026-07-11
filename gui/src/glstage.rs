//! The GL viz stage — real shaders under the canvas. Ping-pong feedback
//! rendertextures (the Milkdrop trick), FFT buckets + waveform + a scrolling
//! spectrum-HISTORY texture uploaded to the GPU, VizBus stats as uniforms.
//! Painter layers + the EQ curve composite over the result.
//!
//! Renders through egui's PaintCallback on the glow backend (GL 3.3 core).
//! VIEWPORT DISCIPLINE (learned the hard way — v0.6.0 painted over the
//! panels): egui_glow sets the viewport to the callback rect and scissor to
//! the clip rect BEFORE invoking us. The offscreen FBO passes disable scissor
//! and use their own viewport; the present pass restores egui's viewport,
//! re-enables scissor, and draws a plain fullscreen triangle — which then
//! fills exactly the canvas rect and clips exactly to egui's clip. Never
//! reproject the rect into window NDC by hand.

use std::time::Instant;

use eframe::glow::{self, HasContext};

/// Spectrum texture width (matches the analyzer's bin count upper bound).
pub const SPEC_W: usize = 64;
/// Waveform texture width (decimated).
pub const WAVE_W: usize = 256;
/// Spectrum-history rows (the audio terrain's depth).
pub const HIST_ROWS: usize = 128;

/// Everything a shader frame needs, by value (Copy) so the paint callback can
/// own it without borrowing App.
#[derive(Clone, Copy)]
pub struct Uniforms {
    pub mode: GlMode,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub pulse: f32,
    pub beat_phase: f32,
    pub bright: f32,
    pub rainbow: bool,
    pub accent: [f32; 3],
    pub spec: [f32; SPEC_W],
    pub wave: [f32; WAVE_W],
}

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms {
            mode: GlMode::Warp,
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            pulse: 0.0,
            beat_phase: 0.0,
            bright: 0.0,
            rainbow: true,
            accent: [0.0, 0.88, 1.0],
            spec: [0.0; SPEC_W],
            wave: [0.0; WAVE_W],
        }
    }
}

/// One variant per fragment program, in `PROGRAM_SRC` order.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GlMode {
    Warp,
    Flame,
    Smoke,
    Plasma,
    Starfield,
    Kaleido,
    Tunnel3d,
    Metaballs,
    Voronoi,
    Nebula,
    Terrain,
    Ripples,
    Julia,
}

pub const MODE_COUNT: usize = 13;

/// Owns GL resource ids only (the context is passed into every call, keeping
/// this Send for the paint callback's Arc<Mutex>).
pub struct GlStage {
    prog: [glow::Program; MODE_COUNT],
    present: glow::Program,
    vao: glow::VertexArray,
    fbo: [glow::Framebuffer; 2],
    tex: [glow::Texture; 2],
    size: (i32, i32),
    spec_tex: glow::Texture,
    wave_tex: glow::Texture,
    hist_tex: glow::Texture,
    hist_row: usize,
    frame: u64,
    ping: usize,
    /// Simulation clock: advanced by CLAMPED real dt per painted frame, so a
    /// stalled frame never fast-forwards the effects on resume (backlog is
    /// dropped, not replayed — same rule as the painter histories).
    sim: f32,
    last_paint: Option<Instant>,
}

const VS_FULL: &str = r#"#version 330 core
const vec2 P[3] = vec2[3](vec2(-1.,-1.), vec2(3.,-1.), vec2(-1.,3.));
out vec2 vUv;
void main() {
    vec2 p = P[gl_VertexID];
    vUv = p * 0.5 + 0.5;
    gl_Position = vec4(p, 0., 1.);
}"#;

/// Shared fragment prelude: uniforms + helpers every mode uses.
const FRAG_COMMON: &str = r#"#version 330 core
in vec2 vUv;
out vec4 frag;
uniform sampler2D uPrev;
uniform sampler2D uSpec;
uniform sampler2D uWave;
uniform sampler2D uHist;
uniform float uHistRow;
uniform vec2  uRes;
uniform float uTime;
uniform float uBass;
uniform float uMid;
uniform float uTreb;
uniform float uPulse;
uniform float uPhase;
uniform float uBright;
uniform int   uRainbow;
uniform vec3  uAccent;

float spec(float x)  { return texture(uSpec, vec2(clamp(x, 0., 1.), .5)).r; }
float wav(float x)   { return texture(uWave, vec2(clamp(x, 0., 1.), .5)).r; }
float hist(vec2 xz)  { return texture(uHist, vec2(clamp(xz.x, 0., 1.), fract(uHistRow - xz.y))).r; }

vec3 hsv2rgb(vec3 c) {
    vec3 p = abs(fract(c.xxx + vec3(0., 2./3., 1./3.)) * 6. - 3.);
    return c.z * mix(vec3(1.), clamp(p - 1., 0., 1.), c.y);
}
float hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}
float vnoise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    f = f * f * (3. - 2. * f);
    return mix(mix(hash(i), hash(i + vec2(1., 0.)), f.x),
               mix(hash(i + vec2(0., 1.)), hash(i + vec2(1., 1.)), f.x), f.y);
}
float fbm(vec2 p) {
    float v = 0., a = .5;
    for (int i = 0; i < 5; i++) { v += a * vnoise(p); p = p * 2.03 + vec2(1.7, 4.1); a *= .5; }
    return v;
}
vec2 aspect(vec2 uv) { return (uv - .5) * vec2(uRes.x / uRes.y, 1.); }
"#;

const FRAG_WARP: &str = r#"
void main() {
    vec2 c = aspect(vUv);
    float ang = .0035 + uBass * .028 + sin(uTime * .13) * .004;
    float zoom = .994 - uPulse * .012;
    float ca = cos(ang), sa = sin(ang);
    vec2 w = mat2(ca, -sa, sa, ca) * c * zoom;
    w.x /= uRes.x / uRes.y;
    vec3 prev = texture(uPrev, w + .5).rgb;
    prev *= .962 + uTreb * .022;
    prev = min(prev, vec3(1.2));
    float r = length(c) * 2.;
    float a = atan(c.y, c.x) / 6.28318 + .5;
    float s = spec(fract(a + uPhase));
    float ring = smoothstep(.045, .0, abs(r - (.38 + s * .5 + uPulse * .06)));
    float w0 = wav(vUv.x) * .5;
    float wv = smoothstep(.02, .0, abs(vUv.y - .5 - w0 * .35));
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(a + uTime * .02, .85, 1.)) : uAccent;
    vec3 add = base * (ring * (.55 + uPulse * .9) + wv * .18);
    frag = vec4(max(prev, add), 1.);
}"#;

const FRAG_FLAME: &str = r#"
vec3 firePal(float h) {
    h = clamp(h, 0., 1.);
    return clamp(vec3(h * 1.6, h * h * 1.25, h * h * h * .9), 0., 1.);
}
void main() {
    vec2 px = 1. / uRes;
    float drift = (vnoise(vUv * 9. + uTime * .9) - .5) * 3.5 * px.x;
    vec2 below = vUv + vec2(drift, -(1.6 + uBass * 2.2) * px.y);
    float heat = texture(uPrev, below).a;
    heat *= .986 - vnoise(vUv * 13. - uTime * 1.7) * .045;
    float src = smoothstep(.05, .0, vUv.y) * spec(vUv.x) * (0.9 + uPulse * 1.6);
    float mid = smoothstep(.12, .0, distance(vUv, vec2(.5, .06))) * uPulse * .8;
    heat = max(heat, clamp(src + mid, 0., 1.));
    vec3 col = (uRainbow == 1)
        ? firePal(heat)
        : mix(vec3(0.), uAccent * (heat * 1.4), smoothstep(.02, .6, heat));
    frag = vec4(col, heat);
}"#;

const FRAG_SMOKE: &str = r#"
vec2 curl(vec2 p) {
    float e = .01;
    return vec2(vnoise(p + vec2(0., e)) - vnoise(p - vec2(0., e)),
                vnoise(p - vec2(e, 0.)) - vnoise(p + vec2(e, 0.))) / (2. * e);
}
void main() {
    vec2 flow = curl(vUv * 3.2 + vec2(0., uTime * .06)) * (.0016 + uMid * .0045);
    flow.y -= .0014 + uBass * .0028;
    float d = texture(uPrev, vUv + flow).a;
    d *= .988;
    float floorSrc = smoothstep(.06, .0, vUv.y) * spec(vUv.x) * (.5 + uBass);
    float burst = smoothstep(.16, .0, distance(vUv, vec2(.5))) * uPulse * .9;
    d = clamp(max(d, floorSrc + burst), 0., 1.);
    float shade = d * (.55 + .45 * vnoise(vUv * 6. + uTime * .12));
    vec3 tint = (uRainbow == 1)
        ? hsv2rgb(vec3(.55 + uBright * .35 + vUv.y * .12, .55, shade))
        : uAccent * shade;
    frag = vec4(tint, d);
}"#;

const FRAG_PLASMA: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    float t = uTime * .6;
    float k = 3. + uBass * 4.;
    float s = sin(p.x * k + t) + sin(p.y * k * 1.3 - t * 1.2)
            + sin((p.x + p.y) * k * .8 + t * .7) + sin(length(p) * k * 1.7 - t * 1.5);
    s = s * .25 + .5;
    float e = spec(fract(s + uPhase * .5));
    vec3 col = (uRainbow == 1) ? hsv2rgb(vec3(s + uTime * .03, .8, .55 + .45 * e))
                               : uAccent * (s * .6 + e * .6);
    frag = vec4(col * (.8 + uPulse * .5), 0.);
}"#;

const FRAG_STARFIELD: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    vec3 col = texture(uPrev, .5 + (vUv - .5) * .985).rgb * .82;
    float speed = .15 + uBass * .9;
    for (int i = 0; i < 40; i++) {
        float fi = float(i);
        vec2 dir = normalize(vec2(hash(vec2(fi, 1.)), hash(vec2(fi, 7.))) - .5);
        float ph = fract(hash(vec2(fi, 3.)) + uTime * speed * (.3 + hash(vec2(fi, 5.))));
        vec2 sp = dir * ph * .8;
        float d = length(p - sp);
        float b = (1. - ph) * .0016 / (d * d + .00002);
        vec3 sc = (uRainbow == 1) ? hsv2rgb(vec3(hash(vec2(fi, 9.)), .6, 1.)) : uAccent;
        col += sc * min(b, .8) * (.5 + uTreb);
    }
    frag = vec4(min(col, vec3(1.4)), 0.);
}"#;

const FRAG_KALEIDO: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    float seg = 6. + floor(uBright * 6.);
    float a = atan(p.y, p.x) + uTime * .05 + uPulse * .1;
    float r = length(p);
    a = abs(mod(a, 6.28318 / seg) - 3.14159 / seg);
    vec2 q = vec2(cos(a), sin(a)) * r;
    q.x /= uRes.x / uRes.y;
    vec3 prev = texture(uPrev, q * .92 + .5).rgb * .96;
    float ring = smoothstep(.03, .0, abs(r - .25 - spec(fract(r + uPhase)) * .35));
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(r + uTime * .04, .85, 1.)) : uAccent;
    frag = vec4(max(prev, base * ring * (.5 + uPulse)), 0.);
}"#;

const FRAG_TUNNEL3D: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    float r = length(p) + 1e-4;
    float a = atan(p.y, p.x) / 6.28318 + .5;
    float z = .25 / r + uTime * (1.2 + uBass * 3.);
    float stripes = spec(fract(a * 3.));
    float rings = smoothstep(.4, .9, sin(z * 6.283) * .5 + .5);
    float wall = stripes * .8 + rings * .4;
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(fract(a + z * .05), .8, 1.)) : uAccent;
    vec3 col = base * wall * smoothstep(.0, .35, r);
    frag = vec4(col * (.7 + uPulse * .6), 0.);
}"#;

const FRAG_METABALLS: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    float f = 0.;
    vec3 col = vec3(0.);
    float e0 = uBass, e1 = uMid, e2 = uTreb, e3 = uPulse, e4 = uBright, e5 = uBass * .5 + uTreb * .5;
    for (int i = 0; i < 6; i++) {
        float fi = float(i);
        float en = (i == 0) ? e0 : (i == 1) ? e1 : (i == 2) ? e2 : (i == 3) ? e3 : (i == 4) ? e4 : e5;
        vec2 c = vec2(sin(uTime * (.3 + fi * .11) + fi * 2.1),
                      cos(uTime * (.23 + fi * .07) + fi * 1.3)) * .32;
        float rr = .02 + en * .06;
        float d = dot(p - c, p - c);
        float g = rr / max(d, 1e-5);
        f += g;
        col += ((uRainbow == 1) ? hsv2rgb(vec3(fi / 6. + uTime * .02, .8, 1.)) : uAccent) * g;
    }
    float m = smoothstep(1., 1.6, f);
    frag = vec4(col / max(f, 1e-3) * m + col * .06, 0.);
}"#;

const FRAG_VORONOI: &str = r#"
void main() {
    vec2 p = vUv * vec2(uRes.x / uRes.y, 1.) * 6.;
    p.y += uTime * .4;
    vec2 ic = floor(p), fpt = fract(p);
    float f1 = 9.;
    vec2 id = ic;
    for (int y = -1; y <= 1; y++) for (int x = -1; x <= 1; x++) {
        vec2 g = vec2(float(x), float(y));
        vec2 o = .5 + .4 * sin(uTime * .8 + 6.28 * vec2(hash(ic + g), hash(ic + g + vec2(7., 3.))));
        float d = length(g + o - fpt);
        if (d < f1) { f1 = d; id = ic + g; }
    }
    float e = spec(fract(hash(id) * .9));
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(hash(id) + uTime * .01, .75, .35 + .65 * e))
                                : uAccent * (.2 + e);
    float edge = smoothstep(.08, .02, f1) * uPulse;
    frag = vec4(base + vec3(edge), 0.);
}"#;

const FRAG_NEBULA: &str = r#"
void main() {
    vec2 p = aspect(vUv) * 2.;
    float t = uTime * .03;
    float n1 = fbm(p * 1.5 + vec2(t, -t * .7));
    float n2 = fbm(p * 3. + vec2(-t * 1.3, t));
    float d = n1 * n2 * 1.8;
    float core = smoothstep(.35, .9, d) * (.6 + uBass);
    vec3 c1 = (uRainbow == 1) ? hsv2rgb(vec3(.68 + uBright * .2, .7, 1.)) : uAccent;
    vec3 c2 = (uRainbow == 1) ? hsv2rgb(vec3(.95, .8, 1.)) : uAccent * .6;
    vec3 col = c1 * d * .5 + c2 * core;
    vec3 prev = texture(uPrev, vUv + vec2(n1 - n2) * .002).rgb;
    frag = vec4(max(col, prev * .94), 0.);
}"#;

/// The audio terrain: tron wireframe mountains raymarched over the REAL
/// spectrum history texture (x = frequency, depth = time).
const FRAG_TERRAIN: &str = r#"
float hgt(vec2 xz) {
    float h = hist(vec2(xz.x * .5 + .5, xz.y * (1. / 16.)));
    return h * (.5 + uPulse * .2);
}
void main() {
    vec2 p = aspect(vUv);
    vec3 ro = vec3(0., .42, 0.);
    vec3 rd = normalize(vec3(p.x, p.y - .16, .8));
    float scroll = uTime * (1.2 + uBass * 2.);
    vec3 col = vec3(0.);
    float glow = smoothstep(.28, .0, abs(rd.y + .02)) * (.22 + uTreb * .55);
    vec3 horizon = (uRainbow == 1) ? hsv2rgb(vec3(uPhase, .7, 1.)) : uAccent;
    col += horizon * glow;
    if (rd.y < -.005) {
        float tt = .05;
        bool hit = false;
        vec3 pos = ro;
        for (int i = 0; i < 48; i++) {
            pos = ro + rd * tt;
            if (pos.y < hgt(vec2(pos.x, pos.z + scroll))) { hit = true; break; }
            tt += .04 + tt * .07;
            if (tt > 7.) break;
        }
        if (hit) {
            float zc = pos.z + scroll;
            vec2 g = vec2(pos.x * 7., zc * 7.);
            vec2 gf = abs(fract(g) - .5);
            float line = smoothstep(.42, .5, max(gf.x, gf.y));
            float h = hgt(vec2(pos.x, zc));
            float fog = exp(-tt * .5);
            vec3 wire = (uRainbow == 1) ? hsv2rgb(vec3(fract(h * 1.2 + uTime * .02), .85, 1.)) : uAccent;
            col = mix(col, wire * (line * 1.25 + h * .8) + vec3(.012), fog);
        }
    }
    frag = vec4(col, 0.);
}"#;

const FRAG_RIPPLES: &str = r#"
void main() {
    vec2 p = aspect(vUv);
    float r = length(p);
    float ring = 0.;
    for (int k = 0; k < 3; k++) {
        float ph = fract(uPhase + float(k) / 3.);
        ring += smoothstep(.05, .0, abs(r - ph * .75)) * (1. - ph);
    }
    vec2 n = p / max(r, 1e-4);
    vec3 prev = texture(uPrev, vUv - n * ring * .02).rgb * .965;
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(r - uTime * .05, .7, 1.)) : uAccent;
    frag = vec4(max(prev, base * ring * (.5 + uBass)), 0.);
}"#;

const FRAG_JULIA: &str = r#"
void main() {
    vec2 p = aspect(vUv) * (2.6 - uPulse * .4);
    vec2 c = vec2(-.745 + sin(uTime * .11) * .09 + uBass * .05,
                  .186 + cos(uTime * .13) * .09);
    vec2 z = p;
    float it = 0.;
    for (int i = 0; i < 48; i++) {
        z = vec2(z.x * z.x - z.y * z.y, 2. * z.x * z.y) + c;
        it = float(i);
        if (dot(z, z) > 4.) break;
    }
    float f = pow(it / 48., .6);
    vec3 col = (uRainbow == 1) ? hsv2rgb(vec3(f * .9 + uTime * .02, .8, f)) : uAccent * f;
    frag = vec4(col * (.75 + uPulse * .5), 0.);
}"#;

const FRAG_PRESENT: &str = r#"#version 330 core
in vec2 vUv;
out vec4 frag;
uniform sampler2D uTex;
void main() { frag = vec4(texture(uTex, vUv).rgb, 1.); }
"#;

/// Fragment sources in `GlMode` order.
const PROGRAM_SRC: [&str; MODE_COUNT] = [
    FRAG_WARP,
    FRAG_FLAME,
    FRAG_SMOKE,
    FRAG_PLASMA,
    FRAG_STARFIELD,
    FRAG_KALEIDO,
    FRAG_TUNNEL3D,
    FRAG_METABALLS,
    FRAG_VORONOI,
    FRAG_NEBULA,
    FRAG_TERRAIN,
    FRAG_RIPPLES,
    FRAG_JULIA,
];

impl GlStage {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let compile = |vs: &str, fs: &str| -> Result<glow::Program, String> {
                let program = gl.create_program()?;
                let stages = [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)];
                let mut shaders = Vec::new();
                for (kind, src) in stages {
                    let sh = gl.create_shader(kind)?;
                    gl.shader_source(sh, src);
                    gl.compile_shader(sh);
                    if !gl.get_shader_compile_status(sh) {
                        return Err(gl.get_shader_info_log(sh));
                    }
                    gl.attach_shader(program, sh);
                    shaders.push(sh);
                }
                gl.link_program(program);
                if !gl.get_program_link_status(program) {
                    return Err(gl.get_program_info_log(program));
                }
                for sh in shaders {
                    gl.detach_shader(program, sh);
                    gl.delete_shader(sh);
                }
                Ok(program)
            };

            let mut progs: Vec<glow::Program> = Vec::with_capacity(MODE_COUNT);
            for (i, frag) in PROGRAM_SRC.iter().enumerate() {
                progs.push(
                    compile(VS_FULL, &format!("{FRAG_COMMON}{frag}"))
                        .map_err(|e| format!("mode {i}: {e}"))?,
                );
            }
            let prog: [glow::Program; MODE_COUNT] =
                progs.try_into().map_err(|_| "prog array".to_string())?;
            let present = compile(VS_FULL, FRAG_PRESENT)?;
            let vao = gl.create_vertex_array()?;

            let mk_tex = || -> Result<glow::Texture, String> {
                let t = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
                Ok(t)
            };

            let spec_tex = mk_tex()?;
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::R32F as i32, SPEC_W as i32, 1, 0,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(None),
            );
            let wave_tex = mk_tex()?;
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::R32F as i32, WAVE_W as i32, 1, 0,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(None),
            );
            let hist_tex = mk_tex()?;
            let zeros = vec![0u8; SPEC_W * HIST_ROWS * 4];
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::R32F as i32, SPEC_W as i32, HIST_ROWS as i32, 0,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(&zeros)),
            );

            let fbo = [gl.create_framebuffer()?, gl.create_framebuffer()?];
            let tex = [mk_tex()?, mk_tex()?];
            gl.bind_texture(glow::TEXTURE_2D, None);

            Ok(GlStage {
                prog,
                present,
                vao,
                fbo,
                tex,
                size: (0, 0),
                spec_tex,
                wave_tex,
                hist_tex,
                hist_row: 0,
                frame: 0,
                ping: 0,
                sim: 0.0,
                last_paint: None,
            })
        }
    }

    /// (Re)size the ping-pong feedback textures to the canvas pixel size.
    unsafe fn ensure_size(&mut self, gl: &glow::Context, w: i32, h: i32) { unsafe {
        if (w, h) == self.size || w <= 0 || h <= 0 {
            return;
        }
        for i in 0..2 {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex[i]));
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::RGBA16F as i32, w, h, 0,
                glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(None),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo[i]));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D,
                Some(self.tex[i]), 0,
            );
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        self.size = (w, h);
    }}

    /// One feedback pass + present, inside egui's paint callback. `vp` is
    /// egui_glow's own viewport for this callback: [left_px, from_bottom_px,
    /// width_px, height_px] (GL bottom-origin).
    pub fn paint(&mut self, gl: &glow::Context, u: &Uniforms, vp: [i32; 4]) {
        unsafe {
            let (w, h) = (vp[2], vp[3]);
            if w < 8 || h < 8 {
                return;
            }
            self.frame = self.frame.wrapping_add(1);
            // Sim clock: clamp dt so frame stalls never fast-forward the FX.
            let now = Instant::now();
            let dt = self
                .last_paint
                .map(|t| now.duration_since(t).as_secs_f32())
                .unwrap_or(1.0 / 60.0)
                .min(0.05);
            self.last_paint = Some(now);
            self.sim += dt;
            let prev_fbo = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
            self.ensure_size(gl, w, h);

            // Upload audio textures (spectrum, waveform, and every other frame
            // one new row of the scrolling spectrum-history for the terrain).
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.spec_tex));
            let spec_bytes: &[u8] =
                core::slice::from_raw_parts(u.spec.as_ptr() as *const u8, SPEC_W * 4);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D, 0, 0, 0, SPEC_W as i32, 1,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(spec_bytes)),
            );
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.wave_tex));
            let wave_bytes: &[u8] =
                core::slice::from_raw_parts(u.wave.as_ptr() as *const u8, WAVE_W * 4);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D, 0, 0, 0, WAVE_W as i32, 1,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(wave_bytes)),
            );
            gl.active_texture(glow::TEXTURE3);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.hist_tex));
            if self.frame % 2 == 0 {
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D, 0, 0, self.hist_row as i32, SPEC_W as i32, 1,
                    glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(spec_bytes)),
                );
                self.hist_row = (self.hist_row + 1) % HIST_ROWS;
            }

            // Offscreen feedback pass. Scissor must be OFF here (egui's window
            // scissor would clip the FBO), blend off (we own every pixel).
            let src = self.ping;
            let dst = 1 - self.ping;
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo[dst]));
            gl.viewport(0, 0, w, h);
            let prog = self.prog[u.mode as usize];
            gl.use_program(Some(prog));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex[src]));
            let loc = |n: &str| gl.get_uniform_location(prog, n);
            gl.uniform_1_i32(loc("uPrev").as_ref(), 0);
            gl.uniform_1_i32(loc("uSpec").as_ref(), 1);
            gl.uniform_1_i32(loc("uWave").as_ref(), 2);
            gl.uniform_1_i32(loc("uHist").as_ref(), 3);
            gl.uniform_1_f32(loc("uHistRow").as_ref(), self.hist_row as f32 / HIST_ROWS as f32);
            gl.uniform_2_f32(loc("uRes").as_ref(), w as f32, h as f32);
            gl.uniform_1_f32(loc("uTime").as_ref(), self.sim);
            gl.uniform_1_f32(loc("uBass").as_ref(), u.bass);
            gl.uniform_1_f32(loc("uMid").as_ref(), u.mid);
            gl.uniform_1_f32(loc("uTreb").as_ref(), u.treble);
            gl.uniform_1_f32(loc("uPulse").as_ref(), u.pulse);
            gl.uniform_1_f32(loc("uPhase").as_ref(), u.beat_phase);
            gl.uniform_1_f32(loc("uBright").as_ref(), u.bright);
            gl.uniform_1_i32(loc("uRainbow").as_ref(), if u.rainbow { 1 } else { 0 });
            gl.uniform_3_f32(loc("uAccent").as_ref(), u.accent[0], u.accent[1], u.accent[2]);
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            // Present: restore egui's framebuffer + ITS viewport (the callback
            // rect) + scissor, then a plain fullscreen triangle fills exactly
            // the canvas rect and clips exactly to egui's clip rect.
            gl.bind_framebuffer(glow::FRAMEBUFFER, if prev_fbo != 0 {
                Some(glow::NativeFramebuffer(
                    std::num::NonZeroU32::new(prev_fbo as u32).unwrap(),
                ))
            } else {
                None
            });
            gl.viewport(vp[0], vp[1], vp[2], vp[3]);
            gl.enable(glow::SCISSOR_TEST);
            gl.use_program(Some(self.present));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex[dst]));
            gl.uniform_1_i32(gl.get_uniform_location(self.present, "uTex").as_ref(), 0);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);

            self.ping = dst;
        }
    }
}
