use sea_orm::DbBackend;

/// SeaORM 只会为 AST 查询按方言生成占位符，手写 `Statement` 的 SQL 原样下发。
/// 手写 SQL 统一写 `?`，连接 PostgreSQL 时在这里换成 `$n`。
///
/// 单引号字符串和 `--` 行注释里的 `?` 原样保留，不会被当成占位符。
pub(crate) fn bind_placeholders(backend: DbBackend, sql: &str) -> String {
    if backend != DbBackend::Postgres {
        return sql.to_string();
    }

    let mut index = 0usize;
    let mut bound = String::with_capacity(sql.len() + 8);
    let mut characters = sql.chars().peekable();
    let mut in_string = false;
    let mut in_line_comment = false;

    while let Some(character) = characters.next() {
        if in_line_comment {
            bound.push(character);
            if character == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_string {
            bound.push(character);
            if character == '\'' {
                if characters.peek() == Some(&'\'') {
                    bound.push(characters.next().expect("peeked character"));
                } else {
                    in_string = false;
                }
            }
            continue;
        }

        match character {
            '\'' => {
                in_string = true;
                bound.push(character);
            }
            '-' if characters.peek() == Some(&'-') => {
                in_line_comment = true;
                bound.push(character);
            }
            '?' => {
                index += 1;
                bound.push('$');
                bound.push_str(&index.to_string());
            }
            _ => bound.push(character),
        }
    }
    bound
}

#[cfg(test)]
mod tests {
    use super::bind_placeholders;
    use sea_orm::DbBackend;

    #[test]
    fn sqlite_keeps_question_mark_placeholders() {
        assert_eq!(
            bind_placeholders(DbBackend::Sqlite, "SELECT 1 WHERE a = ? AND b = ?"),
            "SELECT 1 WHERE a = ? AND b = ?"
        );
    }

    #[test]
    fn postgres_numbers_placeholders_in_order() {
        assert_eq!(
            bind_placeholders(
                DbBackend::Postgres,
                "UPDATE t SET a = ? WHERE b = ? AND c = ?"
            ),
            "UPDATE t SET a = $1 WHERE b = $2 AND c = $3"
        );
    }

    #[test]
    fn postgres_without_placeholders_is_unchanged() {
        assert_eq!(
            bind_placeholders(DbBackend::Postgres, "SELECT 1"),
            "SELECT 1"
        );
    }

    #[test]
    fn postgres_keeps_question_marks_inside_string_literals() {
        assert_eq!(
            bind_placeholders(
                DbBackend::Postgres,
                "SELECT 'a?b', 'it''s ?' FROM t WHERE d = ?"
            ),
            "SELECT 'a?b', 'it''s ?' FROM t WHERE d = $1"
        );
    }

    #[test]
    fn postgres_keeps_question_marks_inside_line_comments() {
        assert_eq!(
            bind_placeholders(DbBackend::Postgres, "SELECT 1 -- 是否 ?\nWHERE a = ?"),
            "SELECT 1 -- 是否 ?\nWHERE a = $1"
        );
    }
}
