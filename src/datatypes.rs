use crate::schema::Schema;
use arrow::array::Array;
use arrow::datatypes;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

pub type BooleanType = datatypes::BooleanType;
pub type I32Type = datatypes::Int32Type;
pub type I64Type = datatypes::Int64Type;
pub type DoubleType = datatypes::Float64Type;
pub type StringType = datatypes::Utf8Type;

#[derive(Debug)]
pub struct ColumnVector {
    pub field: Box<Arc<dyn Array>>,
}

pub struct RecordBatch {
    pub schema: Rc<Schema>,
    pub fields: Vec<Box<ColumnVector>>,
}
