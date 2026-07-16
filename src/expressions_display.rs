use std::fmt;

use crate::expressions::{
    Alias, Binary, BinaryOperator, Column, ColumnIndex, Expression, LiteralDouble, LiteralLong,
    LiteralString, Operator,
};

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Column(c) => write!(f, "#{}", c.name),
            Expression::ColumnIndex(c) => write!(f, "#{}", c.index),
            Expression::Alias(a) => write!(f, "{} as {}", a.expression, a.alias),
            Expression::LiteralString(s) => write!(f, "'{}'", s.value),
            Expression::LiteralDouble(d) => write!(f, "{}", d.value),
            Expression::Aggregate(a) => write!(f, "{}({})", a.operator, a.expression),
            Expression::Binary(b) => write!(f, "{} {} {}", b.L, b.operator, b.R),
        }
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operator::SUM => write!(f, "SUM"),
            Operator::MIN => write!(f, "MIN"),
            Operator::MAX => write!(f, "MAX"),
            Operator::AVG => write!(f, "AVG"),
            Operator::COUNT => write!(f, "COUNT"),
            Operator::AGG => write!(f, "AGG"),
        }
    }
}

impl fmt::Display for BinaryOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOperator::Eq => write!(f, "="),
            BinaryOperator::Neq => write!(f, "!="),
            BinaryOperator::Gt => write!(f, ">"),
            BinaryOperator::GtEq => write!(f, ">="),
            BinaryOperator::Lt => write!(f, "<"),
            BinaryOperator::LtEq => write!(f, "<="),
            BinaryOperator::And => write!(f, "AND"),
            BinaryOperator::Or => write!(f, "OR"),
            BinaryOperator::Add => write!(f, "+"),
            BinaryOperator::Subtract => write!(f, "-"),
            BinaryOperator::Multiply => write!(f, "*"),
            BinaryOperator::Divide => write!(f, "/"),
            BinaryOperator::Modulus => write!(f, "%"),
        }
    }
}

impl fmt::Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.name)
    }
}

impl fmt::Display for ColumnIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.index)
    }
}

impl fmt::Display for Alias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} as {}", self.expression, self.alias)
    }
}

impl fmt::Display for LiteralString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "'{}'", self.value)
    }
}

impl fmt::Display for LiteralDouble {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl fmt::Display for LiteralLong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl fmt::Display for Binary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.L, self.operator, self.R)
    }
}
