use crate::{BinaryOperator, DinocoValue, Expression};

pub fn col(name: &'static str) -> Expression {
    Expression::Column(name)
}

pub trait Filterable {
    fn eq<T: Into<DinocoValue>>(self, value: T) -> Expression;
    fn neq<T: Into<DinocoValue>>(self, value: T) -> Expression;
    fn gt<T: Into<DinocoValue>>(self, value: T) -> Expression;
    fn lt<T: Into<DinocoValue>>(self, value: T) -> Expression;
    fn gte<T: Into<DinocoValue>>(self, value: T) -> Expression;
    fn lte<T: Into<DinocoValue>>(self, value: T) -> Expression;
    fn like<T: Into<DinocoValue>>(self, value: T) -> Expression;
}

impl Filterable for Expression {
    fn eq<T: Into<DinocoValue>>(self, value: T) -> Expression {
        Expression::BinaryOp {
            left: Box::new(self),
            op: BinaryOperator::Eq,
            right: Box::new(Expression::Value(value.into())),
        }
    }

    fn neq<T: Into<DinocoValue>>(self, value: T) -> Expression {
        Expression::BinaryOp {
            left: Box::new(self),
            op: BinaryOperator::Neq,
            right: Box::new(Expression::Value(value.into())),
        }
    }

    fn gt<T: Into<DinocoValue>>(self, value: T) -> Expression {
        Expression::BinaryOp {
            left: Box::new(self),
            op: BinaryOperator::Gt,
            right: Box::new(Expression::Value(value.into())),
        }
    }

    fn lt<T: Into<DinocoValue>>(self, value: T) -> Expression {
        Expression::BinaryOp {
            left: Box::new(self),
            op: BinaryOperator::Lt,
            right: Box::new(Expression::Value(value.into())),
        }
    }

    fn gte<T: Into<DinocoValue>>(self, value: T) -> Expression {
        Expression::BinaryOp {
            left: Box::new(self),
            op: BinaryOperator::Gte,
            right: Box::new(Expression::Value(value.into())),
        }
    }

    fn lte<T: Into<DinocoValue>>(self, value: T) -> Expression {
        Expression::BinaryOp {
            left: Box::new(self),
            op: BinaryOperator::Lte,
            right: Box::new(Expression::Value(value.into())),
        }
    }

    fn like<T: Into<DinocoValue>>(self, value: T) -> Expression {
        Expression::BinaryOp {
            left: Box::new(self),
            op: BinaryOperator::Like,
            right: Box::new(Expression::Value(value.into())),
        }
    }
}
