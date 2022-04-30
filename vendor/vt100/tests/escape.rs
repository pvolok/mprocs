mod helpers;

#[test]
fn deckpam() {
    helpers::fixture("deckpam");
}

#[test]
fn ri() {
    helpers::fixture("ri");
}

#[test]
fn ris() {
    helpers::fixture("ris");
}

#[test]
fn vb() {
    let mut parser = vt100::Parser::default();
    assert_eq!(parser.screen().visual_bell_count(), 0);

    let screen = parser.screen().clone();
    parser.process(b"\x1bg");
    assert_eq!(parser.screen().visual_bell_count(), 1);
    assert_eq!(parser.screen().visual_bell_count(), 1);
    assert_eq!(parser.screen().contents_diff(&screen), b"");
    assert_eq!(parser.screen().bells_diff(&screen), b"\x1bg");

    let screen = parser.screen().clone();
    parser.process(b"\x1bg");
    assert_eq!(parser.screen().visual_bell_count(), 2);
    assert_eq!(parser.screen().contents_diff(&screen), b"");
    assert_eq!(parser.screen().bells_diff(&screen), b"\x1bg");

    let screen = parser.screen().clone();
    parser.process(b"\x1bg\x1bg\x1bg");
    assert_eq!(parser.screen().visual_bell_count(), 5);
    assert_eq!(parser.screen().contents_diff(&screen), b"");
    assert_eq!(parser.screen().bells_diff(&screen), b"\x1bg");

    let screen = parser.screen().clone();
    parser.process(b"foo");
    assert_eq!(parser.screen().visual_bell_count(), 5);
    assert_eq!(parser.screen().contents_diff(&screen), b"foo");
    assert_eq!(parser.screen().bells_diff(&screen), b"");

    let screen = parser.screen().clone();
    parser.process(b"ba\x1bgr");
    assert_eq!(parser.screen().visual_bell_count(), 6);
    assert_eq!(parser.screen().contents_diff(&screen), b"bar");
    assert_eq!(parser.screen().bells_diff(&screen), b"\x1bg");
}

#[test]
fn decsc() {
    helpers::fixture("decsc");
}
