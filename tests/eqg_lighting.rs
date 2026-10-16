//! Confirm the optional public facade exposes the raw lighting reader.
#[cfg(feature = "eqg")]
#[test]
fn lighting_is_available_through_eqg_facade() {
    let bytes = b"EQGP\x01\0\0\0\x44\x33\x22\x11";
    assert_eq!(
        libeq::eqg::identify(bytes),
        Ok(Some(libeq::eqg::FormatHeader::Lighting { sample_count: 1 }))
    );
    let view = libeq::eqg::lighting::parse(bytes).unwrap();
    assert_eq!(view.packed_words().collect::<Vec<_>>(), [0x11223344]);
}
