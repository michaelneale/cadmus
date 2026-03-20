;; Sqlite query execution
(define (sqlite-query (db_path : String) (query : String))
  (bind db_path "data.db")
  (bind query "SELECT 1")
  (sqlite_query))
