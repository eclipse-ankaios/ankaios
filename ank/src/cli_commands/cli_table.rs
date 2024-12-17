// Copyright (c) 2024 Elektrobit Automotive GmbH
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0 which is available at
// https://www.apache.org/licenses/LICENSE-2.0.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.
//
// SPDX-License-Identifier: Apache-2.0

#[cfg(not(test))]
use crate::log::terminal_width;

#[cfg(test)]
fn terminal_width() -> usize {
    80
}

use common::std_extensions::UnreachableOption;
use tabled::{
    settings::{object::Columns, Modify, Padding, Style, Width},
    Table, Tabled,
};

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct CliTableError(String);

impl fmt::Display for CliTableError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Could not create table: {}", self.0)
    }
}

pub struct CliTable<'a, RowType> {
    rows: &'a [RowType],
    table: Table,
}

impl<'a, RowType> CliTable<'a, RowType>
where
    RowType: Tabled,
{
    const TRUNCATED_COLUMN_SUFFIX: &'static str = "...";
    const FIRST_COLUMN_POS: usize = 0;
    const ZERO_PADDING: usize = 0;

    pub fn new(rows: &'a [RowType]) -> Self {
        let table = Table::new(rows);
        Self { rows, table }
    }

    // [impl->swdd~cli-table-provides-default-table-output~2]
    pub fn create_default_table(mut self) -> String {
        self.table = Table::new(self.rows);
        self.style_blank();
        self.disable_surrounding_padding();

        let default_table = self.table.to_string();

        const NEWLINE: &str = "\n";
        let terminal_width = terminal_width();
        let truncated_lines: Vec<&str> = default_table
            .lines()
            .map(|line| {
                if line.len() > terminal_width {
                    &line[..terminal_width]
                } else {
                    line
                }
            })
            .collect();
        truncated_lines.join(NEWLINE)
    }

    // [impl->swdd~cli-table-provides-table-output-with-wrapped-column~1]
    pub fn table_with_wrapped_column_to_remaining_terminal_width(
        mut self,
        column_position: usize,
    ) -> Result<String, CliTableError> {
        self.style_blank();
        self.disable_surrounding_padding();
        let available_column_width = self.column_width_for_terminal(column_position)?;

        self.table.with(
            Modify::new(Columns::single(column_position)).with(Width::wrap(available_column_width)),
        );
        Ok(self.table.to_string())
    }

    // [impl->swdd~cli-table-provides-table-output-with-truncated-column~1]
    pub fn table_with_truncated_column_to_remaining_terminal_width(
        mut self,
        column_position: usize,
    ) -> Result<String, CliTableError> {
        self.style_blank();
        self.disable_surrounding_padding();

        let available_column_width = self.column_width_for_terminal(column_position)?;
        self.table.with(
            Modify::new(Columns::single(column_position)).with(
                Width::truncate(available_column_width).suffix(Self::TRUNCATED_COLUMN_SUFFIX),
            ),
        );
        Ok(self.table.to_string())
    }

    fn style_blank(&mut self) {
        self.table.with(Style::blank());
    }

    fn disable_surrounding_padding(&mut self) {
        let last_column_pos = self.table.count_columns() - 1;

        let first_column_default_padding = self
            .table
            .get_config()
            .get_padding(tabled::grid::config::Entity::Column(Self::FIRST_COLUMN_POS));

        /* Set the left padding of the first and the right padding of the last column to zero
        to align the table content to the full terminal width for better output quality. */
        self.table
            .with(Modify::new(Columns::first()).with(Padding::new(
                Self::ZERO_PADDING,
                first_column_default_padding.right.size,
                first_column_default_padding.top.size,
                first_column_default_padding.bottom.size,
            )));

        let last_column_default_padding = self
            .table
            .get_config()
            .get_padding(tabled::grid::config::Entity::Column(last_column_pos));

        self.table
            .with(Modify::new(Columns::last()).with(Padding::new(
                last_column_default_padding.left.size,
                Self::ZERO_PADDING,
                last_column_default_padding.top.size,
                last_column_default_padding.bottom.size,
            )));
    }

    // [impl->swdd~cli-table-wrapped-truncated-column-width-depends-on-terminal-width~1]
    fn column_width_for_terminal(&self, column_position: usize) -> Result<usize, CliTableError> {
        const DEFAULT_CONTENT_LENGTH: usize = 0;
        let column_name_length = RowType::headers()
            .get(column_position)
            .unwrap_or_unreachable()
            .len();

        let max_content_length = self
            .rows
            .iter()
            .max_by_key(|row| {
                RowType::fields(row)
                    .get(column_position)
                    .unwrap_or_unreachable()
                    .len()
            })
            .map(|row| {
                RowType::fields(row)
                    .get(column_position)
                    .unwrap_or_unreachable()
                    .len()
            })
            .unwrap_or(DEFAULT_CONTENT_LENGTH);

        // the min length shall be the header column name length
        let column_width = max_content_length.max(column_name_length);

        let total_table_width: usize = self.table.total_width();
        let table_width_other_columns =
            total_table_width.checked_sub(column_width).ok_or_else(|| {
                CliTableError(
                    "overflow when calculating table width for other columns.".to_string(),
                )
            })?;

        let terminal_width = terminal_width();

        // check if at least the column name fits in the terminal
        if table_width_other_columns + column_name_length <= terminal_width {
            terminal_width
                .checked_sub(table_width_other_columns)
                .ok_or_else(|| {
                    CliTableError("overflow when calculating remaining terminal width.".to_string())
                })
        } else {
            // no reasonable terminal width left, avoid breaking the column header name formatting
            Err(CliTableError(
                "no reasonable terminal width available".to_string(),
            ))
        }
    }
}

//////////////////////////////////////////////////////////////////////////////
//                 ########  #######    #########  #########                //
//                    ##     ##        ##             ##                    //
//                    ##     #####     #########      ##                    //
//                    ##     ##                ##     ##                    //
//                    ##     #######   #########      ##                    //
//////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::{CliTable, Tabled};

    #[derive(Debug, Tabled, Clone)]
    #[tabled(rename_all = "UPPERCASE")]
    pub struct TestRow {
        #[tabled(rename = "COLUMN 1")]
        pub col1: String,
        pub col2: String,
        #[tabled(rename = "ANOTHER COLUMN3")]
        pub col3: String,
    }

    #[derive(Debug, Tabled, Clone)]
    #[tabled(rename_all = "UPPERCASE")]
    pub struct TestOneRow {
        #[tabled(rename = "COLUMN 1")]
        pub col1: String,
    }

    // [utest->swdd~cli-table-provides-default-table-output~2]
    #[test]
    fn utest_create_default_table_truncated_table_upon_exceeding_line_length() {
        let table_rows = [TestRow {
            col1: "some default name".to_string(),
            col2: "another content".to_string(),
            col3: "info message that exceeds terminal width and leads to truncating of the whole table".to_string(),
        }];

        let table = CliTable::new(&table_rows);
        let table_output = table.create_default_table();
        let expected_table_output = [
            "COLUMN 1            COL2              ANOTHER COLUMN3                           ",
            "some default name   another content   info message that exceeds terminal width a",
        ]
        .join("\n");

        assert_eq!(table_output, expected_table_output);
    }

    // [utest->swdd~cli-table-provides-default-table-output~2]
    #[test]
    fn utest_create_default_table_line_length_equal_to_terminal_length() {
        let table_rows = [TestRow {
            col1: "default name".to_string(),
            col2: "content".to_string(),
            col3: "message to make the row length equal to terminal length".to_string(),
        }];

        let table = CliTable::new(&table_rows);
        let table_output = table.create_default_table();
        let expected_table_output = [
            "COLUMN 1       COL2      ANOTHER COLUMN3                                        ",
            "default name   content   message to make the row length equal to terminal length",
        ]
        .join("\n");

        assert_eq!(table_output, expected_table_output);
    }

    // [utest->swdd~cli-table-provides-table-output-with-truncated-column~1]
    #[test]
    fn utest_table_with_truncated_column_to_remaining_terminal_width() {
        let table_rows = [TestRow {
            col1: "some unwrapped name".to_string(),
            col2: "another unwrapped content".to_string(),
            col3: "some long info message that shall be truncated".to_string(),
        }];

        let truncated_column_position = 2;

        let table = CliTable::new(&table_rows);
        let table_output = table
            .table_with_truncated_column_to_remaining_terminal_width(truncated_column_position)
            .unwrap();
        let expected_table_output_newlines = 1; // truncated column with suffix '...'
        assert_eq!(
            table_output.matches('\n').count(),
            expected_table_output_newlines
        );

        let expected_additional_info_suffix = "...";
        assert_eq!(
            table_output
                .matches(expected_additional_info_suffix)
                .count(),
            1
        );
    }

    // [utest->swdd~cli-table-provides-table-output-with-wrapped-column~1]
    #[test]
    fn utest_table_with_wrapped_column_to_remaining_terminal_width() {
        let table_rows = [TestRow {
            col1: "some unwrapped name".to_string(),
            col2: "another unwrapped content".to_string(),
            col3: "some long info message that shall be wrapped".to_string(),
        }];

        let wrapped_column_position = 2;

        let table = CliTable::new(&table_rows);
        let table_output = table
            .table_with_wrapped_column_to_remaining_terminal_width(wrapped_column_position)
            .unwrap_or_default();

        let expected_table_output_newlines = 2; // because of wrapping the ANOTHER COLUMN3 column
        assert_eq!(
            table_output.matches('\n').count(),
            expected_table_output_newlines
        );
    }

    // [utest->swdd~cli-table-wrapped-truncated-column-width-depends-on-terminal-width~1]
    #[test]
    fn utest_terminal_width_for_column_no_table_entries() {
        let empty_rows: [TestRow; 0] = [];
        let table = CliTable::new(&empty_rows);
        let column_position = 2;
        let expected_terminal_width = Ok(58); // 80 (terminal width) - (37 - 15 (column name 'ANOTHER COLUMN3')) = 58
        assert_eq!(
            table.column_width_for_terminal(column_position),
            expected_terminal_width
        );
    }

    // [utest->swdd~cli-table-wrapped-truncated-column-width-depends-on-terminal-width~1]
    #[test]
    fn utest_terminal_width_for_column_column_name_bigger_than_column_content() {
        let table_rows = [TestRow {
            col1: "some name1".to_string(),
            col2: "text2".to_string(),
            col3: "short".to_string(),
        }];

        let table = CliTable::new(&table_rows);
        let column_position = 2;
        // 80 (terminal width) - (40 (table width) - 15 (column name 'ANOTHER COLUMN3')) = 55
        let expected_terminal_width = Ok(55);
        assert_eq!(
            table.column_width_for_terminal(column_position),
            expected_terminal_width
        );
    }

    // [utest->swdd~cli-table-wrapped-truncated-column-width-depends-on-terminal-width~1]
    #[test]
    fn utest_terminal_width_for_column_no_reasonable_terminal_width_left() {
        let table_rows = [TestRow {
            col1: "some name1".to_string(),
            col2: "text2".repeat(10),
            col3: "medium length message".to_string(),
        }];

        let table = CliTable::new(&table_rows);
        let column_position = 2;

        // not fulfilled: 80 (terminal width) - 15 (column name 'ANOTHER COLUMN3') >= 70 (total table width other columns)
        let table_output_result = table.column_width_for_terminal(column_position);

        assert!(table_output_result.is_err());

        assert!(table_output_result
            .unwrap_err()
            .0
            .contains("no reasonable terminal width"));
    }

    #[test]
    fn utest_default_table_with_only_one_column() {
        let table_row = [TestOneRow {
            col1: "some default content".to_string(),
        }];

        let table_output = CliTable::new(&table_row).create_default_table();

        let expected_table_output = ["COLUMN 1            ", "some default content"].join("\n");

        assert_eq!(expected_table_output, table_output);
    }
}
