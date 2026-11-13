//! Cross-checks our writer against the independent Ruffle `swf` parser.
//!
//! If this test passes, the emitted bytes are accepted by a production SWF
//! reader written by a different project. It still does not prove visual
//! fidelity in the Adobe projector; see `docs/FEASIBILITY.md`.

use nova_swf::shape::Shape;
use nova_swf::tags::Tag;
use nova_swf::types::{Color, Matrix, PlaceObject};
use nova_swf::{Compression, Movie};

#[test]
fn ruffle_parser_accepts_shapes_sprites_and_timeline() {
    let mut movie = Movie::new_avm2(550.0, 400.0, 24.0, 10);
    movie.set_background(Color::rgb(16, 18, 24));
    movie.tags.push(Tag::DefineShape {
        id: 1,
        shape: Shape::rect(120.0, 80.0, Color::rgb(245, 166, 35)),
    });
    movie.tags.push(Tag::DefineShape {
        id: 2,
        shape: Shape::ellipse(60.0, 60.0, Color::rgba(61, 214, 196, 180)),
    });
    movie.tags.push(Tag::DefineSprite {
        id: 3,
        frame_count: 1,
        tags: vec![
            Tag::PlaceObject2(
                PlaceObject::new(1)
                    .character(2)
                    .matrix(Matrix::from_transform(1.0, 1.0, 0.0, 0.0, 0.0)),
            ),
            Tag::ShowFrame,
        ],
    });
    for frame in 0..3u16 {
        movie.tags.push(Tag::PlaceObject2(
            PlaceObject::new(1)
                .character(1)
                .matrix(Matrix::from_transform(
                    1.0,
                    1.0,
                    12.0,
                    100.0 + frame as f32 * 10.0,
                    100.0,
                ))
                .named("Titel"),
        ));
        movie.tags.push(Tag::ShowFrame);
    }

    let bytes = movie.to_bytes(Compression::Zlib).unwrap();
    let buffer = swf::decompress_swf(bytes.as_slice()).expect("swf crate decompresses file");
    let parsed = swf::parse_swf(&buffer).expect("swf crate parses tag stream");

    assert_eq!(parsed.header.version(), 10);
    assert_eq!(parsed.header.num_frames(), 3);
    assert!(parsed.header.is_action_script_3());
    let frame_rate: f32 = parsed.header.frame_rate().into();
    assert_eq!(frame_rate, 24.0);

    assert!(
        parsed
            .tags
            .iter()
            .any(|tag| matches!(tag, swf::Tag::DefineShape(_))),
        "shape tag was not recognized"
    );
    assert!(
        parsed
            .tags
            .iter()
            .any(|tag| matches!(tag, swf::Tag::DefineSprite(_))),
        "sprite tag was not recognized"
    );
    assert_eq!(
        parsed
            .tags
            .iter()
            .filter(|tag| matches!(tag, swf::Tag::PlaceObject(_)))
            .count(),
        3
    );
}

#[test]
fn ruffle_parser_accepts_empty_document() {
    let movie = Movie::new_avm2(320.0, 240.0, 12.0, 9);
    let bytes = movie.to_bytes(Compression::None).unwrap();
    let buffer = swf::decompress_swf(bytes.as_slice()).unwrap();
    let parsed = swf::parse_swf(&buffer).unwrap();
    assert_eq!(parsed.header.num_frames(), 0);
    assert_eq!(parsed.header.version(), 9);
}

#[test]
fn ruffle_parser_accepts_doabc2_and_symbol_class() {
    let mut movie = Movie::new_avm2(320.0, 240.0, 24.0, 10);
    // Header-only ABC is enough to verify tag framing; the AVM2 parser is
    // tested separately in nova-as.
    let abc = vec![0x10, 0x00, 0x2e, 0x00];
    movie.tags.push(Tag::DoAbc2 {
        flags: 1,
        name: "app".into(),
        data: abc.clone(),
    });
    movie.tags.push(Tag::SymbolClass {
        links: vec![(0, "Main".into())],
    });
    movie.tags.push(Tag::ShowFrame);

    let bytes = movie.to_bytes(Compression::Zlib).unwrap();
    let buffer = swf::decompress_swf(bytes.as_slice()).expect("swf crate decompresses file");
    let parsed = swf::parse_swf(&buffer).expect("swf crate parses tag stream");
    assert_eq!(parsed.header.version(), 10);
    let Some(swf::Tag::DoAbc2(do_abc)) = parsed
        .tags
        .iter()
        .find(|tag| matches!(tag, swf::Tag::DoAbc2(_)))
    else {
        panic!("DoAbc2 tag was not recognized");
    };
    assert!(do_abc.flags.contains(swf::DoAbc2Flag::LAZY_INITIALIZE));
    assert_eq!(do_abc.name.as_bytes(), b"app");
    assert_eq!(do_abc.data, abc.as_slice());
    assert!(parsed.tags.iter().any(|tag| matches!(
        tag,
        swf::Tag::SymbolClass(links)
            if links.len() == 1
                && links[0].id == 0
                && links[0].class_name.as_bytes() == b"Main"
    )));
}
