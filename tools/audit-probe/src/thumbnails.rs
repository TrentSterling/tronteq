//! Bake representative scene frames using the production renderer and synthetic audio.
use crate::{glstage::*, show::ShowMode};
use eframe::glow::{self, HasContext};

pub unsafe fn render(gl: &glow::Context) -> Result<(), String> {
    const W: u32 = 256;
    const H: u32 = 144;
    const COLS: u32 = 6;
    let scenes: Vec<_> = ShowMode::ALL.into_iter().skip(1).collect();
    let rows = (scenes.len() as u32).div_ceil(COLS);
    let mut atlas = image::RgbImage::new(W * COLS, H * rows);
    let tex = gl.create_texture()?;
    gl.bind_texture(glow::TEXTURE_2D, Some(tex));
    gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        W as i32,
        H as i32,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(None),
    );
    let fbo = gl.create_framebuffer()?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(
        glow::FRAMEBUFFER,
        glow::COLOR_ATTACHMENT0,
        glow::TEXTURE_2D,
        Some(tex),
        0,
    );
    if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
        return Err("thumbnail framebuffer incomplete".into());
    }
    let mut stage = GlStage::new(gl)?;
    for (index, scene) in scenes.iter().enumerate() {
        for frame in 0..180 {
            let t = frame as f32 / 60.0;
            let u = Uniforms {
                mode: scene.gl_mode().unwrap(),
                bass: 0.55 + (t * 3.0).sin() * 0.2,
                mid: 0.4,
                treble: 0.3,
                pulse: (t * 6.0).sin().max(0.0) * 0.7,
                loud: 0.6,
                bright: 0.55,
                beat_phase: (t * 2.0).fract(),
                bpm: 120.0,
                bpm_conf: 1.0,
                flux: 0.4,
                crest: 0.6,
                width: 0.4,
                corr: 0.7,
                spec: std::array::from_fn(|i| (i as f32 * 0.21 + t).sin().abs() * 0.6 + 0.08),
                wave: std::array::from_fn(|i| (i as f32 * 0.085 + t).sin() * 0.5),
                ..Default::default()
            };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.scissor(0, 0, W as i32, H as i32);
            stage.paint_elapsed(gl, &u, [0, 0, W as i32, H as i32], 1.0 / 60.0);
        }
        let mut pixels = vec![0u8; (W * H * 4) as usize];
        gl.read_pixels(
            0,
            0,
            W as i32,
            H as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut pixels)),
        );
        if gl.get_error() != glow::NO_ERROR {
            return Err(format!("GL error rendering {}", scene.label()));
        }
        if !pixels
            .chunks_exact(4)
            .any(|p| p[..3].iter().any(|v| *v > 8))
        {
            return Err(format!("empty thumbnail: {}", scene.label()));
        }
        let x0 = index as u32 % COLS * W;
        let y0 = index as u32 / COLS * H;
        for y in 0..H {
            for x in 0..W {
                let p = ((H - 1 - y) * W + x) as usize * 4;
                atlas.put_pixel(
                    x0 + x,
                    y0 + y,
                    image::Rgb([pixels[p], pixels[p + 1], pixels[p + 2]]),
                );
            }
        }
        println!("THUMBNAIL {}", scene.as_str());
    }
    atlas
        .save("target/scene-atlas.png")
        .map_err(|e| e.to_string())?;
    let metadata = serde_json::json!({ "columns": COLS, "tile_width": W, "tile_height": H,
        "scenes": scenes.iter().map(|s| s.as_str()).collect::<Vec<_>>() });
    std::fs::write(
        "target/scene-atlas.json",
        serde_json::to_string_pretty(&metadata).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    gl.delete_framebuffer(fbo);
    gl.delete_texture(tex);
    println!("ATLAS 37/37 production scenes: target/scene-atlas.png");
    Ok(())
}
