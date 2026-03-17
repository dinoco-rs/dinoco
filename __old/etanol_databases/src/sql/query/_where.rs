use crate::Value;

#[derive(Debug, Clone)]
pub enum ModelWhere<T: Value> {
    Equal(&'static str, T),
    NotEqual(&'static str, T),
    GreaterThan(&'static str, T),
    GreaterThanOrEqual(&'static str, T),
    LessThan(&'static str, T),
    LessThanOrEqual(&'static str, T),
    Contains(&'static str, T),
}

impl<T: Value> ModelWhere<T> {
    pub fn sql(&self, position: usize) -> String {
        match self {
            ModelWhere::Equal(name, ..) => format!("{} = ?{}", name, position),
            ModelWhere::NotEqual(name, ..) => format!("{} != ?{}", name, position),
            ModelWhere::GreaterThan(name, ..) => format!("{} > ?{}", name, position),
            ModelWhere::GreaterThanOrEqual(name, ..) => format!("{} >= ?{}", name, position),
            ModelWhere::LessThan(name, ..) => format!("{} < ?{}", name, position),
            ModelWhere::LessThanOrEqual(name, ..) => format!("{} <= ?{}", name, position),
            ModelWhere::Contains(name, ..) => format!("{} LIKE ?{}", name, position),
        }
    }

    pub fn key(&self) -> String {
        match self {
            ModelWhere::Equal(key, ..)
            | ModelWhere::NotEqual(key, ..)
            | ModelWhere::GreaterThan(key, ..)
            | ModelWhere::GreaterThanOrEqual(key, ..)
            | ModelWhere::LessThan(key, ..)
            | ModelWhere::LessThanOrEqual(key, ..)
            | ModelWhere::Contains(key, ..) => key.to_string(),
        }
    }

    pub fn value(&self) -> String {
        match self {
            ModelWhere::Equal(_, v)
            | ModelWhere::NotEqual(_, v)
            | ModelWhere::GreaterThan(_, v)
            | ModelWhere::GreaterThanOrEqual(_, v)
            | ModelWhere::LessThan(_, v)
            | ModelWhere::LessThanOrEqual(_, v) => v.toValue(None),
            ModelWhere::Contains(_, v) => {
                let value = v.toValue(None);

                if &value != "None" {
                    return format!("%{}%", value);
                }
                value
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct WhereQuery {
    pub fields: Vec<String>,
    pub table: String,
    pub values: Vec<String>,
}

impl WhereQuery {
    pub fn new(table: String) -> Self {
        Self {
            table,
            fields: vec![],
            values: vec![],
        }
    }

    pub fn field<T: Value>(&mut self, value: ModelWhere<T>) -> &mut Self {
        self.fields.push(value.sql(self.values.len() + 1));
        self.values.push(value.value());

        self
    }

    pub fn params(&self) -> Vec<String> {
        if self.values.len() > 0 {
            let mut params = vec![];

            for value in self.values.clone() {
                params.push(value);
            }

            return params;
        }

        return vec![];
    }

    pub fn sql(&self) -> String {
        let mut sql = String::new();

        if self.fields.len() > 0 {
            sql = format!("WHERE ");

            let mut wheres = vec![];

            for value in self.fields.clone() {
                wheres.push(value);
            }

            sql.push_str(&wheres.join(" AND "));
        }

        return sql;
    }
}
