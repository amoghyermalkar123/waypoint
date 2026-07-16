use crate::datatypes::{ColumnVector, RecordBatch};
use crate::schema::{Field, Schema};
use anyhow::Result;
use arrow::array::StringArray;
use arrow::compute::cast;
use arrow::datatypes::DataType;
use csv::{Position, Reader, StringRecord};
use std::cell::RefCell;
use std::fs::File;
use std::ops::Index;
use std::rc::Rc;

/// DataSource is a marker trait for any type that supports iterating over a source
/// which contains column oriented data and producing results as a RecordBatch
/// which has column-oriented pickled fields against a given schema
///
/// The datasource today are - Memory, CSV and Parquet
pub trait DataSource {}

impl DataSource for Csv {}

pub struct Csv {
    batch_size: u32,
    pub path: String,
    offset: Position,
    reader: Reader<File>,
    schema: Rc<Schema>,
    col_indices: Vec<usize>,
}

impl Csv {
    pub fn new(
        path: &String,
        projections: &[&str],
        batch_size: u32,
        schema: Rc<Schema>,
    ) -> Result<Self> {
        let mut reader = Reader::from_path(path)?;

        // figure out column indexes
        let headers = reader.headers()?;
        let mut col_indices = Vec::new();
        headers.iter().enumerate().for_each(|enit| {
            if projections.contains(&enit.1) {
                col_indices.push(enit.0);
            }
        });

        Ok(Csv {
            batch_size,
            path: path.clone(),
            offset: reader.position().clone(),
            reader,
            schema,
            col_indices,
        })
    }
}

impl Iterator for Csv {
    type Item = RecordBatch;

    fn next(&mut self) -> Option<Self::Item> {
        let mut cvs = Vec::new();

        self.reader.seek(self.offset.clone());

        let mut records: Vec<StringRecord> = self
            .reader
            .records()
            .by_ref()
            .take(self.batch_size as usize)
            .map(|r| r.expect("failed to get record"))
            .collect();

        if records.is_empty() {
            return None;
        }

        self.offset = self.reader.position().clone();

        for idx in &self.col_indices {
            let mut colv = Vec::with_capacity(records.len());
            for r in &records {
                colv.push(
                    r.get(idx.clone())
                        .expect("no entry found in record at {*idx}"),
                );
            }

            let f = self
                .schema
                .fields
                .get(idx.clone())
                .expect("expected the {idx} column to have a field schema");

            let sarray = StringArray::from(colv);
            let c =
                cast(&sarray, &f.datatype).expect("failed to cast array to arrow datatype array");

            cvs.push(Box::new(ColumnVector { field: Box::new(c) }));
        }

        Some(RecordBatch {
            schema: self.schema.clone(),
            fields: cvs,
        })
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Ok;

    use super::*;

    #[test]
    fn test_iter() -> anyhow::Result<()> {
        let schema = Schema {
            fields: vec![
                Field {
                    name: "id".to_string(),
                    datatype: DataType::Int64,
                },
                Field {
                    name: "state".to_string(),
                    datatype: DataType::Utf8,
                },
                Field {
                    name: "first_name".to_string(),
                    datatype: DataType::Utf8,
                },
                Field {
                    name: "salary".to_string(),
                    datatype: DataType::Float64,
                },
            ],
        };

        // test
        let csv_ds = Csv::new(&"employees.csv".to_string(), &["state"], 3, Rc::new(schema))?;
        let batches: Vec<RecordBatch> = csv_ds.collect();
        assert_eq!(4, batches.len());
        Ok(())
    }
}
