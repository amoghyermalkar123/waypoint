use crate::schema;
use anyhow::Result;
use arrow::datatypes::DataType;
use sqlparser::ast;
use std::hash::{Hash, Hasher};

/// An Expression is an arrangement of symbols used to denote
/// a single SQL object of computation such Aggregates like SUM, MIN
/// or Column Names such as age, name or Binary operations such as
/// Equals, Not Equals, Greater Than, etc.
/// To evaluate an expression means to find a numerical value equivalent to the expression.
#[derive(Debug, PartialEq, Clone, Eq, Hash)]
pub enum Expression {
    Aggregate(Aggregate),
    Binary(Binary),
    Column(Column),
    ColumnIndex(ColumnIndex),
    Alias(Alias),
    LiteralString(LiteralString),
    LiteralDouble(LiteralDouble),
}

impl Expression {
    /// to_field converts any given expression to a `Field` type.
    /// the main reason to do this is for it's further usage in Schema
    /// and logical plan computations. The Fields are going to be the
    /// primary source of information of our Schema knowledge.
    ///
    /// `input` is the schema of the child relation — Column / ColumnIndex
    /// resolve name and type by looking themselves up there.
    pub fn to_field(&self, input: &schema::Schema) -> Result<schema::Field> {
        match self {
            Expression::Aggregate(a) => {
                // COUNT is always Int32; other aggregates inherit the inner expr type
                let datatype = if matches!(a.operator, Operator::COUNT) {
                    DataType::Int32
                } else {
                    a.expression.to_field(input)?.datatype
                };
                Ok(schema::Field {
                    name: a.operator.to_string(),
                    datatype,
                })
            }
            Expression::Binary(b) => {
                // Comparisons / boolean ops → Boolean; math ops keep the left operand's type
                let datatype = match b.operator {
                    BinaryOperator::Eq
                    | BinaryOperator::Neq
                    | BinaryOperator::Gt
                    | BinaryOperator::GtEq
                    | BinaryOperator::Lt
                    | BinaryOperator::LtEq
                    | BinaryOperator::And
                    | BinaryOperator::Or => DataType::Boolean,
                    BinaryOperator::Add
                    | BinaryOperator::Subtract
                    | BinaryOperator::Multiply
                    | BinaryOperator::Divide
                    | BinaryOperator::Modulus => b.L.to_field(input)?.datatype,
                };
                Ok(schema::Field {
                    name: b.operator.to_string(),
                    datatype,
                })
            }
            Expression::Column(c) => {
                input
                    .fields
                    .iter()
                    .find(|f| f.name == c.name)
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "No column named '{}' in {:?}",
                            c.name,
                            input.fields.iter().map(|f| &f.name).collect::<Vec<_>>()
                        )
                    })
            }
            Expression::ColumnIndex(c) => {
                input.fields.get(c.index).cloned().ok_or_else(|| {
                    anyhow::anyhow!(
                        "column index {} out of range (0..{})",
                        c.index,
                        input.fields.len().saturating_sub(1)
                    )
                })
            }
            Expression::Alias(a) => Ok(schema::Field {
                name: a.alias.clone(),
                datatype: a.expression.to_field(input)?.datatype,
            }),
            Expression::LiteralString(s) => Ok(schema::Field {
                name: s.value.clone(),
                datatype: DataType::Utf8,
            }),
            Expression::LiteralDouble(d) => Ok(schema::Field {
                name: d.value.to_string(),
                datatype: DataType::Float64,
            }),
        }
    }
}

/// Aggregate expressions represent symbols such as SUM/MIN/MAX(column_name)
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Aggregate {
    pub operator: Operator,
    pub expression: Box<Expression>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum Operator {
    SUM,
    MIN,
    MAX,
    AVG,
    COUNT,
    AGG,
}

impl Operator {
    pub fn from_str(op: &str) -> Option<Operator> {
        match op {
            "SUM" => Some(Operator::SUM),
            "MIN" => Some(Operator::MIN),
            "MAX" => Some(Operator::MAX),
            "AVG" => Some(Operator::AVG),
            "COUNT" => Some(Operator::COUNT),
            "AGG" => Some(Operator::AGG),
            _ => None,
        }
    }
}
/// Binary expressions represent a 2 operand 1 operator expression
/// such as column_name * 2, etc.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Binary {
    pub operator: BinaryOperator,
    pub L: Box<Expression>,
    pub R: Box<Expression>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum BinaryOperator {
    Eq,
    Neq,
    Gt,
    GtEq,
    Lt,
    LtEq,
    And,
    Or,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulus,
}

impl BinaryOperator {
    pub fn from_sql_binary_op(binop: &ast::BinaryOperator) -> BinaryOperator {
        match binop {
            ast::BinaryOperator::Eq => BinaryOperator::Eq,
            ast::BinaryOperator::NotEq => BinaryOperator::Neq,
            ast::BinaryOperator::Gt => BinaryOperator::Gt,
            ast::BinaryOperator::GtEq => BinaryOperator::GtEq,
            ast::BinaryOperator::Lt => BinaryOperator::Lt,
            ast::BinaryOperator::LtEq => BinaryOperator::LtEq,
            ast::BinaryOperator::And => BinaryOperator::And,
            ast::BinaryOperator::Or => BinaryOperator::Or,
            ast::BinaryOperator::Plus => BinaryOperator::Add,
            ast::BinaryOperator::Minus => BinaryOperator::Subtract,
            ast::BinaryOperator::Multiply => BinaryOperator::Multiply,
            ast::BinaryOperator::Divide => BinaryOperator::Divide,
            ast::BinaryOperator::Modulo => BinaryOperator::Modulus,
            _ => unimplemented!("unsupported binary operator received {}", binop),
        }
    }
}

/// A basic column expression which represents a column_name expression
/// in SQL
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Column {
    pub name: String,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct ColumnIndex {
    pub index: usize,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Alias {
    pub expression: Box<Expression>,
    pub alias: String,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct LiteralString {
    pub value: String,
}

/// f64 cannot derive Eq/Hash (NaN breaks reflexivity). We compare/hash by
/// bit pattern so Expression can still be used in HashMaps/HashSets.
#[derive(Debug, Clone)]
pub struct LiteralDouble {
    pub value: f64,
}

impl PartialEq for LiteralDouble {
    fn eq(&self, other: &Self) -> bool {
        self.value.to_bits() == other.value.to_bits()
    }
}

impl Eq for LiteralDouble {}

impl Hash for LiteralDouble {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.to_bits().hash(state);
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct LiteralLong {
    pub value: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::DataType;

    #[test]
    fn column_to_field_resolves_from_input_schema() {
        let input = schema::Schema {
            fields: vec![schema::Field {
                name: "salary".to_string(),
                datatype: DataType::Float64,
            }],
        };
        let expr = Expression::Column(Column {
            name: "salary".to_string(),
        });
        let field = expr.to_field(&input).expect("to_field");
        assert_eq!("salary", field.name);
        assert_eq!(DataType::Float64, field.datatype);
    }
}
