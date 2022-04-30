mod helpers;

#[test]
fn bel() {
    let mut parser = vt100::Parser::default();
    assert_eq!(parser.screen().audible_bell_count(), 0);

    let screen = parser.screen().clone();
    parser.process(b"\x07");
    assert_eq!(parser.screen().audible_bell_count(), 1);
    assert_eq!(parser.screen().audible_bell_count(), 1);
    assert_eq!(parser.screen().contents_diff(&screen), b"");
    assert_eq!(parser.screen().bells_diff(&screen), b"\x07");

    let screen = parser.screen().clone();
    parser.process(b"\x07");
    assert_eq!(parser.screen().audible_bell_count(), 2);
    assert_eq!(parser.screen().contents_diff(&screen), b"");
    assert_eq!(parser.screen().bells_diff(&screen), b"\x07");

    let screen = parser.screen().clone();
    parser.process(b"\x07\x07\x07");
    assert_eq!(parser.screen().audible_bell_count(), 5);
    assert_eq!(parser.screen().contents_diff(&screen), b"");
    assert_eq!(parser.screen().bells_diff(&screen), b"\x07");

    let screen = parser.screen().clone();
    parser.process(b"foo");
    assert_eq!(parser.screen().audible_bell_count(), 5);
    assert_eq!(parser.screen().contents_diff(&screen), b"foo");
    assert_eq!(parser.screen().bells_diff(&screen), b"");

    let screen = parser.screen().clone();
    parser.process(b"ba\x07r");
    assert_eq!(parser.screen().audible_bell_count(), 6);
    assert_eq!(parser.screen().contents_diff(&screen), b"bar");
    assert_eq!(parser.screen().bells_diff(&screen), b"\x07");
}

#[test]
fn bs() {
    helpers::fixture("bs");
}

#[test]
fn tab() {
    helpers::fixture("tab");
}

#[test]
fn lf() {
    helpers::fixture("lf");
}

#[test]
fn vt() {
    helpers::fixture("vt");
}

#[test]
fn ff() {
    helpers::fixture("ff");
}

#[test]
fn cr() {
    helpers::fixture("cr");
}
