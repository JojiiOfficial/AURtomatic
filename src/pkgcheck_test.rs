use super::*;

#[test]
fn check_unwrap_multi_line() {
    let inp = r#" sha256sums=('0f9ffd30d769e25e091a87b9dda4d688c19bf85b1e1fcb3b89eaae5ff780182a'
  '04917e3cd4307d8e31bfb0027a5dce6d086edb10ff8a716024fbb8bb0c7dccf1'
                '68fc13ed0b7b461f49a9b419af92fedfe6b2db21f61f8ce62f00dfa36cb03ed2'
        '14738b9336285fb7a250ff793e6d069510798c5aa07e93d157f775bf9f07b88f')"#;

    let expect = r#"sha256sums=('0f9ffd30d769e25e091a87b9dda4d688c19bf85b1e1fcb3b89eaae5ff780182a' '04917e3cd4307d8e31bfb0027a5dce6d086edb10ff8a716024fbb8bb0c7dccf1' '68fc13ed0b7b461f49a9b419af92fedfe6b2db21f61f8ce62f00dfa36cb03ed2' '14738b9336285fb7a250ff793e6d069510798c5aa07e93d157f775bf9f07b88f')"#;

    assert_eq!(unwrap_multi_line(inp, "'\n"), expect);
}

#[test]
fn check_is_diff_empty() {
    let d = vec![
        diff::Result::Left("left add"),
        diff::Result::Both("same", "same"),
    ];

    assert!(is_diff_empty(&d));
}

#[test]
fn check_parse_src_1() {
    let inp = "validpgpkeys=('key1', 'key2'); echo 1\n";
    let expect = "validpgpkeys=('key1', 'key2');\n echo 1\n";

    assert_eq!(parse_src_file(inp.to_owned()), expect);
}

#[test]
fn check_parse_src_2() {
    let inp = "# Maintainer Jojii <Jojii@gmx.net>\necho 1";
    let expect = "echo 1\n";

    assert_eq!(parse_src_file(inp.to_owned()), expect);
}

#[test]
fn check_diff_1() {
    let old = fs::read_to_string("./tests/pkgbuild_old").unwrap();
    let new = fs::read_to_string("./tests/pkgbuild_new").unwrap();

    assert_ne!(old, new);

    let a_content = parse_src_file(old.to_owned());
    let b_content = parse_src_file(new.to_owned());

    println!("{}", a_content);

    let diff = diff::lines(a_content.as_str(), b_content.as_str());

    assert!(!is_diff_empty(&diff));
    assert!(Check::check_diff(diff))
}

#[test]
fn hash_file_diff_1() {
    let a = Path::new("./tests/pkgbuild_new");
    let output = hash_file_diff(&a, &a);

    assert!(output.is_ok());
    assert!(output.unwrap());
}

#[test]
fn hash_file_diff_2() {
    let a = Path::new("./tests/pkgbuild_new");
    let b = Path::new("./tests/pkgbuild_old");
    let output = hash_file_diff(&a, &b);

    assert!(output.is_ok());
    assert!(!output.unwrap());
}

#[test]
fn test_get_mime() {
    let mime = get_mime(Path::new("./tests/pkgbuild_new"));

    assert!(mime.is_ok());
    assert_eq!(mime.unwrap(), "text/plain");
}

#[test]
fn check_partial_contains() {
    assert!(partial_contains(ALLOWED_MIMES, "image/jpg"));
    assert!(!partial_contains(ALLOWED_MIMES, "application/xml"));

    assert!(partial_contains(UTF8_MIMES, "application/xml"));
    assert!(partial_contains(UTF8_MIMES, "application/x-desktop"));
}
