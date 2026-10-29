use harfrust::{Direction, Feature, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};

const PT_SANS: &[u8] = include_bytes!("fonts/rb_custom/PT_Sans-Caption-Web-Regular.ttf");

fn positions(
    font: &[u8],
    text: &str,
    clusters: &[u32],
    boundaries: &[u32],
    direction: Direction,
    features: &[Feature],
) -> Vec<[i32; 4]> {
    let font = FontRef::new(font).unwrap();
    let data = ShaperData::new(&font);
    let shaper = data.shaper(&font).build();
    let mut buffer = UnicodeBuffer::new();
    for (index, character) in text.chars().enumerate() {
        buffer.add(
            character,
            clusters.get(index).copied().unwrap_or(index as u32),
        );
    }
    buffer.set_direction(direction);
    buffer.guess_segment_properties();
    shaper
        .shape(
            buffer,
            ShapeOptions::new()
                .features(features)
                .disable_pair_kerning_at(boundaries),
        )
        .glyph_positions()
        .iter()
        .map(|p| [p.x_advance, p.y_advance, p.x_offset, p.y_offset])
        .collect()
}

#[test]
fn selected_pair_keeps_adjacent_kerning() {
    let glyphs = positions(PT_SANS, "AVAV", &[], &[2], Direction::LeftToRight, &[]);
    let advances: Vec<_> = glyphs.iter().map(|pos| pos[0]).collect();
    assert_eq!(advances, [600, 628, 600, 628]);
}

#[test]
fn cluster_identity_survives_direction_and_merged_source_ranges() {
    for direction in [Direction::LeftToRight, Direction::RightToLeft] {
        let natural = positions(PT_SANS, "AVAV", &[0, 0, 20, 30], &[], direction, &[]);
        assert_eq!(
            positions(
                PT_SANS,
                "AVAV",
                &[0, 0, 20, 30],
                &[0, 1, 19],
                direction,
                &[]
            ),
            natural
        );
        let selected = positions(PT_SANS, "AVAV", &[0, 0, 20, 30], &[20], direction, &[]);
        assert_ne!(selected, natural);
        assert_eq!(
            selected.iter().map(|p| p[0]).sum::<i32>() - natural.iter().map(|p| p[0]).sum::<i32>(),
            45
        );
    }
}

#[test]
fn legacy_pair_keeps_incoming_adjustments() {
    let font = include_bytes!("fonts/text-rendering-tests/TestKERNOne.otf");
    let glyphs = positions(font, "uTu", &[], &[2], Direction::LeftToRight, &[]);
    let geometry: Vec<_> = glyphs.iter().map(|p| (p[0], p[2])).collect();
    assert_eq!(geometry, [(300, 0), (500, -100), (400, 0)]);
}

#[test]
fn non_kerning_pair_features_keep_their_global_mask() {
    let features = ["test".parse().unwrap()];
    for font in [
        include_bytes!("fonts/aots/gpos2_1_simple_f1.otf").as_slice(),
        include_bytes!("fonts/aots/gpos2_2_font1.otf").as_slice(),
    ] {
        let text = "\u{11}\u{12}\u{13}\u{11}\u{12}\u{14}";
        let natural = positions(font, text, &[], &[], Direction::LeftToRight, &features);
        assert_eq!(
            positions(
                font,
                text,
                &[],
                &[1, 2, 3, 4, 5],
                Direction::LeftToRight,
                &features
            ),
            natural
        );
        assert_ne!(
            positions(font, text, &[], &[], Direction::LeftToRight, &[]),
            natural
        );
    }
}

fn pair_feature_font(source: &[u8], tags: &[[u8; 4]], required: bool) -> Vec<u8> {
    fn u16_at(data: &[u8], offset: usize) -> usize {
        u16::from_be_bytes(data[offset..offset + 2].try_into().unwrap()) as usize
    }
    fn u32_at(data: &[u8], offset: usize) -> usize {
        u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize
    }
    let record = (12..12 + 16 * u16_at(source, 4))
        .step_by(16)
        .find(|&offset| &source[offset..offset + 4] == b"GPOS")
        .unwrap();
    let start = u32_at(source, record + 8);
    let mut gpos = source[start..start + u32_at(source, record + 12)].to_vec();
    let old_features = u16_at(&gpos, 6);
    assert_eq!(u16_at(&gpos, old_features), 1);
    let feature = old_features + u16_at(&gpos, old_features + 6);
    let feature = gpos[feature..feature + 4 + 2 * u16_at(&gpos, feature + 2)].to_vec();
    let features = gpos.len() as u16;
    let count = tags.len() as u16;
    gpos.extend(count.to_be_bytes());
    for tag in tags {
        gpos.extend(tag);
        gpos.extend((2 + 6 * count).to_be_bytes());
    }
    gpos.extend(feature);
    let scripts = gpos.len() as u16;
    // One DFLT script with a default language system and the selected features.
    gpos.extend([0, 1, b'D', b'F', b'L', b'T', 0, 8, 0, 4, 0, 0, 0, 0]);
    gpos.extend(if required { 0u16 } else { u16::MAX }.to_be_bytes());
    gpos.extend(count.to_be_bytes());
    for index in 0..count {
        gpos.extend(index.to_be_bytes());
    }
    gpos[4..6].copy_from_slice(&scripts.to_be_bytes());
    gpos[6..8].copy_from_slice(&features.to_be_bytes());
    let mut font = source.to_vec();
    font[record + 8..record + 12].copy_from_slice(&(source.len() as u32).to_be_bytes());
    font[record + 12..record + 16].copy_from_slice(&(gpos.len() as u32).to_be_bytes());
    font.extend(gpos);
    font
}

#[test]
fn pair_lookup_provenance_preserves_shared_and_required_features() {
    let text = "\u{11}\u{12}\u{13}\u{11}\u{12}\u{14}";
    for source in [
        include_bytes!("fonts/aots/gpos2_1_simple_f1.otf").as_slice(),
        include_bytes!("fonts/aots/gpos2_2_font1.otf").as_slice(),
    ] {
        for (shared, required) in [(false, false), (true, false), (false, true)] {
            let tags = [*b"kern", *b"test"];
            let font = pair_feature_font(source, &tags[..if shared { 2 } else { 1 }], required);
            let features = ["test".parse().unwrap()];
            let natural = positions(&font, text, &[], &[], Direction::LeftToRight, &features);
            assert_ne!(
                natural,
                positions(source, text, &[], &[], Direction::LeftToRight, &[])
            );
            let selected = positions(
                &font,
                text,
                &[],
                &[1, 2, 3, 4, 5],
                Direction::LeftToRight,
                &features,
            );
            if shared || required {
                assert_eq!(selected, natural);
            } else {
                assert_ne!(selected, natural);
            }
        }
    }
}

#[test]
fn ligature_removed_boundary_does_not_disable_the_next_pair() {
    let natural = positions(PT_SANS, "fiAV", &[], &[], Direction::LeftToRight, &[]);
    assert_eq!(natural.len(), 3);
    assert_eq!(
        positions(PT_SANS, "fiAV", &[], &[1], Direction::LeftToRight, &[]),
        natural
    );
}

#[test]
fn suppressed_second_value_record_does_not_skip_the_outgoing_pair() {
    let font = pair_feature_font(
        include_bytes!("fonts/aots/gpos2_1_next_glyph_f1.otf"),
        &[*b"kern"],
        false,
    );
    let selected = positions(
        &font,
        "\u{12}\u{12}\u{12}\u{12}",
        &[],
        &[1],
        Direction::LeftToRight,
        &[],
    );
    assert_eq!(
        selected.iter().map(|p| (p[2], p[3])).collect::<Vec<_>>(),
        [(0, 0), (-100, 0), (0, -100), (0, 0)]
    );
}

#[test]
fn disjoint_non_kerning_features_do_not_preserve_a_shared_pair() {
    let font = pair_feature_font(
        include_bytes!("fonts/aots/gpos2_1_simple_f1.otf"),
        &[*b"kern", *b"test", *b"tst2"],
        false,
    );
    let text = "\u{12}\u{13}";
    let features = ["test[0:1]".parse().unwrap(), "tst2[1:2]".parse().unwrap()];
    let natural = positions(&font, text, &[], &[], Direction::LeftToRight, &features);
    let selected = positions(&font, text, &[], &[1], Direction::LeftToRight, &features);
    assert_ne!(selected, natural);
    assert_eq!(
        selected,
        positions(&font, text, &[], &[1], Direction::LeftToRight, &[])
    );
}

#[test]
fn shared_feature_values_preserve_pair_positioning() {
    let font = pair_feature_font(
        include_bytes!("fonts/aots/gpos2_1_simple_f1.otf"),
        &[*b"kern", *b"test"],
        false,
    );
    let text = "\u{12}\u{13}";
    let features = [
        "test[0:1]=1".parse().unwrap(),
        "test[1:2]=2".parse().unwrap(),
    ];
    assert_eq!(
        positions(&font, text, &[], &[1], Direction::LeftToRight, &features),
        positions(&font, text, &[], &[], Direction::LeftToRight, &features)
    );
}
