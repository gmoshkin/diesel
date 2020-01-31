use std::collections::HashMap;
use std::ffi::CStr;

use super::{Binds, Statement, StatementMetadata};
use crate::mysql::{Mysql, MysqlType, MysqlValue};
use crate::result::QueryResult;
use crate::row::*;

pub struct StatementIterator<'a> {
    stmt: &'a mut Statement,
    output_binds: Binds,
}

#[allow(clippy::should_implement_trait)] // don't neet `Iterator` here
impl<'a> StatementIterator<'a> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(stmt: &'a mut Statement, types: Vec<Option<MysqlType>>) -> QueryResult<Self> {
        let mut output_binds = if types.iter().any(Option::is_none) {
            let metadata = stmt.metadata()?;
            Binds::from_output_types(types, Some(metadata.fields()))
        } else {
            Binds::from_output_types(types, None)
        };

        stmt.execute_statement(&mut output_binds)?;

        Ok(StatementIterator { stmt, output_binds })
    }

    pub fn map<F, T>(mut self, mut f: F) -> QueryResult<Vec<T>>
    where
        F: FnMut(MysqlRow) -> QueryResult<T>,
    {
        let mut results = Vec::new();
        while let Some(row) = self.next() {
            results.push(f(row?)?);
        }
        Ok(results)
    }

    fn next(&mut self) -> Option<QueryResult<MysqlRow>> {
        match self.stmt.populate_row_buffers(&mut self.output_binds) {
            Ok(Some(())) => Some(Ok(MysqlRow {
                col_idx: 0,
                binds: &mut self.output_binds,
                stmt: &self.stmt,
            })),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

pub struct MysqlRow<'a> {
    col_idx: usize,
    binds: &'a Binds,
    stmt: &'a Statement,
}

impl<'a> Row<Mysql> for MysqlRow<'a> {
    fn take(&mut self) -> Option<MysqlValue<'_>> {
        let current_idx = self.col_idx;
        self.col_idx += 1;
        self.binds.field_data(current_idx)
    }

    fn next_is_null(&self, count: usize) -> bool {
        (0..count).all(|i| self.binds.field_data(self.col_idx + i).is_none())
    }

    fn column_count(&self) -> usize {
        self.binds.len()
    }

    fn column_name(&self) -> Option<&str> {
        let metadata = self
            .stmt
            .metadata()
            .expect("Failed to get result metadata from the mysql backend");
        let field = if self.col_idx == 0 {
            metadata.fields()[0]
        } else {
            metadata.fields()[self.col_idx - 1]
        };
        unsafe {
            Some(CStr::from_ptr(field.name).to_str().expect(
                "Diesel assumes that your mysql database uses the \
                 utf8mb4 encoding. That's not the case if you hit \
                 this error message.",
            ))
        }
    }
}

pub struct NamedStatementIterator<'a> {
    stmt: &'a mut Statement,
    output_binds: Binds,
    metadata: StatementMetadata,
}

#[allow(clippy::should_implement_trait)] // don't need `Iterator` here
impl<'a> NamedStatementIterator<'a> {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(stmt: &'a mut Statement) -> QueryResult<Self> {
        let metadata = stmt.metadata()?;
        let mut output_binds = Binds::from_result_metadata(metadata.fields());

        stmt.execute_statement(&mut output_binds)?;

        Ok(NamedStatementIterator {
            stmt,
            output_binds,
            metadata,
        })
    }

    pub fn map<F, T>(mut self, mut f: F) -> QueryResult<Vec<T>>
    where
        F: FnMut(NamedMysqlRow) -> QueryResult<T>,
    {
        let mut results = Vec::new();
        while let Some(row) = self.next() {
            results.push(f(row?)?);
        }
        Ok(results)
    }

    fn next(&mut self) -> Option<QueryResult<NamedMysqlRow>> {
        match self.stmt.populate_row_buffers(&mut self.output_binds) {
            Ok(Some(())) => Some(Ok(NamedMysqlRow {
                binds: &self.output_binds,
                column_indices: self.metadata.column_indices(),
            })),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

pub struct NamedMysqlRow<'a> {
    binds: &'a Binds,
    column_indices: &'a HashMap<&'a str, usize>,
}

impl<'a> NamedRow<Mysql> for NamedMysqlRow<'a> {
    fn index_of(&self, column_name: &str) -> Option<usize> {
        self.column_indices.get(column_name).cloned()
    }

    fn get_raw_value(&self, idx: usize) -> Option<MysqlValue<'_>> {
        self.binds.field_data(idx)
    }
}
