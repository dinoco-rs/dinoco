use std::sync::Mutex;

/// The one-column projection of `field`, as the `'static` field list a query
/// needs. Generated models always find the column in their own `fields`, so
/// nothing is allocated; any other name is interned once per distinct name.
pub(crate) fn single_field(field: &'static str, fields: &'static [&'static str]) -> &'static [&'static str] {
    if let Some(index) = fields.iter().position(|candidate| *candidate == field) {
        return &fields[index..=index];
    }

    static INTERNED: Mutex<Vec<&'static [&'static str]>> = Mutex::new(Vec::new());
    let mut interned = INTERNED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(projection) = interned.iter().find(|projection| projection[0] == field) {
        return projection;
    }

    let projection: &'static [&'static str] = Box::leak(Box::new([field]));
    interned.push(projection);
    projection
}

#[cfg(test)]
mod tests {
    use super::single_field;

    const FIELDS: &[&str] = &["id", "email", "name"];

    #[test]
    fn model_columns_are_sliced_from_the_model_fields() {
        let projection = single_field("email", FIELDS);

        assert_eq!(projection, ["email"]);
        assert!(std::ptr::eq(projection.as_ptr(), &FIELDS[1]));
    }

    #[test]
    fn other_columns_are_interned_once() {
        let first = single_field("dinoco_projection_test_column", FIELDS);
        let second = single_field("dinoco_projection_test_column", FIELDS);

        assert_eq!(first, ["dinoco_projection_test_column"]);
        assert!(std::ptr::eq(first, second));
    }
}
