use super::*;

#[test]
fn observation_writer_checks_the_budget_before_adding_a_chunk() {
    let mut output = BoundedBytes {
        bytes: Vec::new(),
        maximum: 3,
    };
    output.write_all(b"ab").unwrap();
    assert!(output.write_all(b"cd").is_err());
    assert_eq!(output.bytes, b"ab");
    output.write_all(b"c").unwrap();
    assert_eq!(output.bytes, b"abc");
}
