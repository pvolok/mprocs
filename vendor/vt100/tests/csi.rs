mod helpers;

#[test]
fn absolute_movement() {
    helpers::fixture("absolute_movement");
}

#[test]
fn row_clamp() {
    let mut vt = vt100::Parser::default();
    assert_eq!(vt.screen().cursor_position(), (0, 0));
    vt.process(b"\x1b[15d");
    assert_eq!(vt.screen().cursor_position(), (14, 0));
    vt.process(b"\x1b[150d");
    assert_eq!(vt.screen().cursor_position(), (23, 0));
}

#[test]
fn relative_movement() {
    helpers::fixture("relative_movement");
}

#[test]
fn ed() {
    helpers::fixture("ed");
}

#[test]
fn el() {
    helpers::fixture("el");
}

#[test]
fn ich_dch_ech() {
    helpers::fixture("ich_dch_ech");
}

#[test]
fn il_dl() {
    helpers::fixture("il_dl");
}

#[test]
fn scroll() {
    helpers::fixture("scroll");
}
