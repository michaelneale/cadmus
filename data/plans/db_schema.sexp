;; Inspect db schema
(define (db-schema (db_path : String))
  (bind db_path "data.db")
  (sqlite_schema))
