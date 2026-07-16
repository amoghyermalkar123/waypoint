use arrow::datatypes;

pub struct Schema {
    pub fields: Vec<Field>,
}

impl Schema {
    fn new(fields: Vec<Field>) -> Self {
        Schema { fields }
    }
}

/// TODO: consider an optimization here where we can avoid cloning fields
/// the Clone trait is mainly added because the schema() method in dataframe.rs
/// needs owned fields to create new schema structs
#[derive(PartialEq, Clone)]
pub struct Field {
    pub name: String,
    pub datatype: datatypes::DataType,
}
