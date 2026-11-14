//! Generates the six Mega-Milestone-9 demo `.nova` projects into `samples/`.
//!
//! Run with `cargo run -p nova-core --example gen_demos`. The files are
//! ordinary version-2 documents and must keep loading without the generator.

use nova_core::{
    Asset, Easing, Frame, Layer, LayerKind, ObjectInstance, ObjectKind, PathCommand, PathShape,
    Project, Rgba, Scene, SoundCue, Symbol, SymbolKind, Tween, TweenKind,
};
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("samples");
    std::fs::create_dir_all(&out).unwrap();

    nested_symbols_demo(&out);
    movieclip_animation_demo(&out);
    bitmap_assets_demo(&out);
    audio_timeline_demo(&out);
    masks_and_tweens_demo(&out);
    multi_scene_demo(&out);

    println!("Demo-Projekte unter {} geschrieben", out.display());
}

fn base(name: &str, width: f32, height: f32) -> Project {
    let mut project = Project::new_empty(name);
    let scene = project.active_scene_mut().unwrap();
    scene.width = width;
    scene.height = height;
    scene.fps = 24.0;
    project
}

fn layer_with(name: &str, frames: usize) -> Layer {
    let mut layer = Layer::new(name, frames);
    layer.frames[0].keyframe = true;
    layer
}

fn put(scene: &mut Scene, layer_index: usize, frame: usize, object: ObjectInstance) {
    scene.ensure_frame_count(frame + 1);
    let layer = &mut scene.layers[layer_index];
    layer.ensure_frames(frame + 1);
    if !layer.frames[frame].keyframe {
        layer.frames[frame].keyframe = true;
        if frame > 0 {
            if let Some(prev) = layer.last_keyframe_at_or_before(frame - 1) {
                layer.frames[frame].objects = layer.frames[prev].objects.clone();
            }
        }
    }
    layer.frames[frame].objects.push(object);
}

fn rect(name: &str, x: f32, y: f32, w: f32, h: f32, color: Rgba) -> ObjectInstance {
    ObjectInstance::rect(name, x, y, w, h, color)
}

fn ellipse(name: &str, x: f32, y: f32, w: f32, h: f32, color: Rgba) -> ObjectInstance {
    ObjectInstance::ellipse(name, x, y, w, h, color)
}

fn symbol_instance(name: &str, x: f32, y: f32, symbol: nova_core::SymbolId) -> ObjectInstance {
    ObjectInstance::symbol(name, x, y, symbol, Rgba::WHITE)
}

fn save(project: &Project, dir: &std::path::Path, file: &str) {
    project.save(&dir.join(file)).unwrap();
    println!("  {file}");
}

/// 1. Nested symbols: a MovieClip wheel containing a nested graphic spoke,
///    instanced twice on the main scene with different scales.
fn nested_symbols_demo(dir: &std::path::Path) {
    let mut project = base("M9 Nested Symbols", 640.0, 360.0);

    let mut spoke = Symbol::with_kind(
        "Speiche",
        SymbolKind::Graphic,
        16.0,
        90.0,
        Rgba::rgb(120, 200, 255),
    );
    spoke.timeline.layers[0].frames[0].objects.push(rect(
        "Stab",
        0.0,
        -45.0,
        8.0,
        90.0,
        Rgba::rgb(230, 240, 255),
    ));

    let mut wheel = Symbol::with_kind(
        "Rad",
        SymbolKind::MovieClip,
        120.0,
        120.0,
        Rgba::rgba(0, 0, 0, 0),
    );
    let mut rim_layer = layer_with("Felge", 1);
    rim_layer.frames[0].objects.push(ellipse(
        "Reifen",
        0.0,
        0.0,
        120.0,
        120.0,
        Rgba::rgb(60, 70, 90),
    ));
    rim_layer.frames[0].objects.push(ellipse(
        "Nabe",
        0.0,
        0.0,
        30.0,
        30.0,
        Rgba::rgb(200, 210, 230),
    ));
    wheel.timeline.layers.push(rim_layer);
    for index in 0..4 {
        let mut spoke_instance = symbol_instance("Speiche", 0.0, 0.0, spoke.id);
        spoke_instance.transform.rotation = index as f32 * 90.0;
        put(&mut wheel.timeline, 1, 0, spoke_instance);
    }
    let spoke_id = spoke.id;
    let wheel_id = wheel.id;
    let _ = spoke_id;
    project.symbols.push(spoke);
    project.symbols.push(wheel);

    let scene = project.active_scene_mut().unwrap();
    scene.layers.push(layer_with("Hintergrund", 60));
    scene.layers.push(layer_with("Raeder", 60));
    let mut big = symbol_instance("Grosses Rad", 180.0, 180.0, wheel_id);
    big.transform.scale_x = 1.6;
    big.transform.scale_y = 1.6;
    put(scene, 1, 0, big);
    let mut small = symbol_instance("Kleines Rad", 460.0, 140.0, wheel_id);
    small.transform.scale_x = 0.7;
    small.transform.scale_y = 0.7;
    put(scene, 2, 0, small);
    put(
        scene,
        2,
        0,
        rect("Boden", 320.0, 320.0, 640.0, 20.0, Rgba::rgb(40, 48, 66)),
    );

    project.metadata.name = "M9 Nested Symbols".into();
    project.metadata.description =
        "MovieClip-Rad mit verschachtelter Grafik-Speiche, zweimal instanziiert.".into();
    project.validate().unwrap();
    save(&project, dir, "m9-nested-symbols.nova");
}

/// 2. MovieClip with its own multi-frame timeline: a bouncing ball inside the
///    clip, the clip itself tweened across the main scene.
fn movieclip_animation_demo(dir: &std::path::Path) {
    let mut project = base("M9 MovieClip Animation", 640.0, 360.0);

    let mut ball = Symbol::with_kind(
        "Ball",
        SymbolKind::MovieClip,
        60.0,
        60.0,
        Rgba::rgba(0, 0, 0, 0),
    );
    let mut track = layer_with("Sprung", 24);
    track.frames[0].objects.push(ellipse(
        "Kugel",
        0.0,
        -120.0,
        50.0,
        50.0,
        Rgba::rgb(245, 166, 35),
    ));
    track.frames[0].tween = Some(Tween {
        easing: Easing::Linear,
        kind: TweenKind::Motion,
    });
    // Duplicate the keyframe content at frame 12 with a moved object.
    let mut mid = Frame::default();
    mid.keyframe = true;
    mid.objects = track.frames[0].objects.clone();
    for object in &mut mid.objects {
        object.transform.y = 120.0;
    }
    track.frames[11] = mid;
    // End keyframe back at the top.
    let mut end = Frame::default();
    end.keyframe = true;
    end.objects = track.frames[0].objects.clone();
    track.frames[23] = end;
    ball.timeline.layers.clear();
    ball.timeline.layers.push(track);
    let ball_id = ball.id;
    project.symbols.push(ball);

    let scene = project.active_scene_mut().unwrap();
    scene.layers.push(layer_with("Buehne", 48));
    let mut instance = symbol_instance("BouncingBall", 320.0, 180.0, ball_id);
    instance.transform.scale_x = 1.4;
    instance.transform.scale_y = 1.4;
    put(scene, 1, 0, instance.clone());
    // Motion tween the instance across the stage.
    let mut far = instance.clone();
    far.transform.x = 500.0;
    far.transform.y = 120.0;
    far.transform.rotation = 180.0;
    scene.ensure_frame_count(48);
    let layer = &mut scene.layers[1];
    layer.ensure_frames(48);
    layer.frames[23].keyframe = true;
    layer.frames[23].objects = vec![far];
    let mut far_late = instance.clone();
    far_late.transform.x = 180.0;
    far_late.transform.y = 260.0;
    layer.frames[35].keyframe = true;
    layer.frames[35].objects = vec![far_late];
    layer.frames[0].tween = Some(Tween {
        easing: Easing::EaseInOut,
        kind: TweenKind::Motion,
    });
    // Shadow ellipse under the ball path.
    scene.layers.push(layer_with("Schatten", 48));
    put(
        scene,
        2,
        0,
        ellipse(
            "Schatten",
            320.0,
            320.0,
            90.0,
            14.0,
            Rgba::rgba(0, 0, 0, 90),
        ),
    );

    project.metadata.name = "M9 MovieClip Animation".into();
    project.metadata.description =
        "MovieClip mit eigener 24-Frame-Timeline plus Motion-Tween auf der Bühne.".into();
    project.validate().unwrap();
    save(&project, dir, "m9-movieclip-animation.nova");
}

fn checker_png() -> Vec<u8> {
    let mut image = image::RgbaImage::new(64, 64);
    for y in 0..64u32 {
        for x in 0..64u32 {
            let on = ((x / 8) + (y / 8)) % 2 == 0;
            let color = if on {
                image::Rgba([70, 120, 200, 255])
            } else {
                image::Rgba([240, 244, 250, 255])
            };
            image.put_pixel(x, y, color);
        }
    }
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn sine_wav() -> Vec<u8> {
    let sample_rate = 22050u32;
    let channels = 1u16;
    let duration = 1.0f32;
    let samples = (sample_rate as f32 * duration) as usize;
    let data_size = (samples * 2) as u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&(channels * 2).to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for index in 0..samples {
        let t = index as f32 / sample_rate as f32;
        let value = (t * 440.0 * std::f32::consts::TAU).sin() * 0.4;
        let sample = (value * i16::MAX as f32) as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

/// 3. Bitmap asset pipeline: embedded PNG placed on stage, scaled and moved
///    by a motion tween.
fn bitmap_assets_demo(dir: &std::path::Path) {
    let mut project = base("M9 Bitmap Assets", 640.0, 360.0);
    let png = checker_png();
    project.assets.push(Asset::bitmap(
        "Raster",
        "assets/raster.png",
        "image/png",
        64,
        64,
        png,
    ));

    let asset_id = project.assets[0].id;
    let scene = project.active_scene_mut().unwrap();
    scene.layers.push(layer_with("Bilder", 36));
    let mut image = ObjectInstance::bitmap("Raster gross", 200.0, 160.0, asset_id, 128.0, 128.0);
    image.kind = ObjectKind::Bitmap {
        asset: asset_id,
        width: 128.0,
        height: 128.0,
        crop: None,
    };
    put(scene, 1, 0, image);
    let mut image2 = ObjectInstance::bitmap("Raster klein", 440.0, 240.0, asset_id, 48.0, 48.0);
    image2.kind = ObjectKind::Bitmap {
        asset: asset_id,
        width: 48.0,
        height: 48.0,
        crop: None,
    };
    put(scene, 1, 18, image2);
    // Motion tween needs a matching end keyframe with the same object id:
    // `put` copied the frame-0 objects into frame 18, so retarget the copied
    // "Raster gross" instance to its end pose.
    for object in scene.layers[1].frames[18].objects.iter_mut() {
        if object.name == "Raster gross" {
            object.transform.x = 440.0;
            object.transform.y = 240.0;
            object.kind = ObjectKind::Bitmap {
                asset: asset_id,
                width: 64.0,
                height: 64.0,
                crop: None,
            };
        }
    }
    let layer = &mut scene.layers[1];
    layer.frames[0].tween = Some(Tween {
        easing: Easing::Linear,
        kind: TweenKind::Motion,
    });

    project.metadata.name = "M9 Bitmap Assets".into();
    project.metadata.description =
        "Eingebettetes PNG-Asset, zwei Platzierungen, Motion-Tween.".into();
    project.validate().unwrap();
    save(&project, dir, "m9-bitmap-assets.nova");
}

/// 4. Audio timeline: embedded WAV on a dedicated sound layer with an event
///    cue plus a looping timeline cue.
fn audio_timeline_demo(dir: &std::path::Path) {
    let mut project = base("M9 Audio Timeline", 640.0, 360.0);
    project.assets.push(Asset::audio(
        "Sinuston",
        "assets/tone.wav",
        "audio/wav",
        22050,
        1,
        1.0,
        sine_wav(),
    ));

    let asset_id = project.assets[0].id;
    let scene = project.active_scene_mut().unwrap();
    scene.layers.push(layer_with("Optik", 72));
    put(
        scene,
        0,
        0,
        rect("Rahmen", 320.0, 180.0, 500.0, 300.0, Rgba::rgb(30, 36, 52)),
    );
    put(
        scene,
        0,
        0,
        ellipse("Punkt", 320.0, 180.0, 60.0, 60.0, Rgba::rgb(245, 166, 35)),
    );

    let mut sound_layer = Layer::new("Tonspur", 72);
    sound_layer.kind = LayerKind::Sound;
    sound_layer.frames[0].keyframe = true;
    sound_layer.frames[0].sound = Some(SoundCue {
        asset: asset_id,
        offset: 0.0,
        duration: 1.0,
        event: true,
        loop_count: 1,
    });
    sound_layer.frames[36].keyframe = true;
    sound_layer.frames[36].sound = Some(SoundCue {
        asset: asset_id,
        offset: 0.0,
        duration: 0.0,
        event: false,
        loop_count: 2,
    });
    scene.layers.push(sound_layer);

    project.metadata.name = "M9 Audio Timeline".into();
    project.metadata.description =
        "Eingebettetes WAV-Asset mit Event- und Timeline-Cue auf einer Sound-Ebene.".into();
    project.validate().unwrap();
    save(&project, dir, "m9-audio-timeline.nova");
}

/// 5. Mask + guide layers and a shape tween between two rectangles.
fn masks_and_tweens_demo(dir: &std::path::Path) {
    let mut project = base("M9 Masks & Tweens", 640.0, 360.0);

    let scene = project.active_scene_mut().unwrap();
    // Guide layer: visible in the editor, skipped on export/playback.
    let mut guide = layer_with("Hilfslinien", 48);
    guide.kind = LayerKind::Guide;
    guide.frames[0].objects.push(rect(
        "Zentrierung",
        320.0,
        0.0,
        2.0,
        360.0,
        Rgba::rgb(255, 255, 255),
    ));
    scene.layers.push(guide);

    // Content layer sits directly below the mask in the layer stack: the SWF
    // exporter clips the layer at index-1 relative to the mask.
    scene.layers.push(layer_with("Inhalt", 48));

    // Mask layer above the content layer.
    let mut mask_layer = layer_with("Maske", 48);
    mask_layer.kind = LayerKind::Mask;
    mask_layer.frames[0]
        .objects
        .push(ellipse("Loch", 320.0, 180.0, 220.0, 140.0, Rgba::WHITE));
    scene.layers.push(mask_layer);

    scene.layers.push(layer_with("Form-Tween", 48));

    put(
        scene,
        2,
        0,
        rect("Panel", 320.0, 180.0, 500.0, 300.0, Rgba::rgb(80, 140, 220)),
    );
    put(
        scene,
        2,
        0,
        ellipse(
            "Akzent",
            320.0,
            180.0,
            160.0,
            160.0,
            Rgba::rgb(245, 166, 35),
        ),
    );

    // Shape tween: rectangle morphing to ellipse-compatible rectangle.
    let mut start = rect("Morph", 120.0, 260.0, 80.0, 80.0, Rgba::rgb(120, 220, 140));
    start.kind = ObjectKind::Path {
        width: 80.0,
        height: 80.0,
        paths: vec![PathShape {
            fill: Rgba::rgb(120, 220, 140),
            commands: vec![
                PathCommand::MoveTo { x: -40.0, y: -40.0 },
                PathCommand::LineTo { x: 40.0, y: -40.0 },
                PathCommand::LineTo { x: 40.0, y: 40.0 },
                PathCommand::LineTo { x: -40.0, y: 40.0 },
                PathCommand::LineTo { x: -40.0, y: -40.0 },
            ],
        }],
    };
    put(scene, 4, 0, start);

    // Keep identity stable: clone the placed object and move its centre.
    // Same id is required for shape-tween pairing.
    let placed_id = scene.layers[4].frames[0].objects[0].id;
    let mut end_clone = scene.layers[4].frames[0].objects[0].clone();
    end_clone.transform.x = 500.0;
    end_clone.transform.rotation = 45.0;
    end_clone.id = placed_id;
    scene.layers[4].frames[24].keyframe = true;
    scene.layers[4].frames[24].objects = vec![end_clone];
    scene.layers[4].frames[0].tween = Some(Tween {
        easing: Easing::Linear,
        kind: TweenKind::Shape,
    });

    project.metadata.name = "M9 Masks & Tweens".into();
    project.metadata.description =
        "Hilfsebene, Maskenebene und Form-Tween zwischen zwei Keyframes.".into();
    project.validate().unwrap();
    save(&project, dir, "m9-masks-tweens.nova");
}

/// 6. Multi-scene regression: two scenes, each with its own layers.
fn multi_scene_demo(dir: &std::path::Path) {
    let mut project = base("M9 Multi Scene", 640.0, 360.0);
    project.scenes[0].name = "Intro".into();

    let mut outro = Scene::new("Outro", 640.0, 360.0);
    outro.background = Rgba::rgb(18, 20, 26);
    outro.fps = 24.0;
    outro.layers.push(layer_with("Endcard", 30));
    put(
        &mut outro,
        0,
        0,
        rect("Karte", 320.0, 180.0, 480.0, 240.0, Rgba::rgb(40, 48, 66)),
    );
    put(
        &mut outro,
        0,
        0,
        ellipse("Punkt", 320.0, 120.0, 60.0, 60.0, Rgba::rgb(245, 166, 35)),
    );
    project.scenes.push(outro);

    let intro = project.active_scene_mut().unwrap();
    intro.layers.push(layer_with("Titel", 30));
    put(
        intro,
        1,
        0,
        rect("Balken", 320.0, 180.0, 400.0, 8.0, Rgba::rgb(245, 166, 35)),
    );
    put(
        intro,
        1,
        0,
        ellipse("Marke", 320.0, 140.0, 40.0, 40.0, Rgba::rgb(120, 200, 255)),
    );

    project.metadata.name = "M9 Multi Scene".into();
    project.metadata.description = "Zwei Szenen mit eigenen Ebenen und Keyframes.".into();
    project.validate().unwrap();
    save(&project, dir, "m9-multi-scene.nova");
}
