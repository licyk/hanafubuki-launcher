use super::*;

#[test]
fn history_evicts_oldest() {
    let mut history = BoundedHistory::new(2);
    assert_eq!(history.push(1), None);
    history.push(2);
    assert_eq!(history.push(3), Some(1));
    assert_eq!(history.iter().copied().collect::<Vec<_>>(), vec![2, 3]);
}
