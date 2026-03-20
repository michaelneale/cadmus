;; Query database with SQL
(define (query-database (db_path : String) (query : String))
  (bind db_path "data.db")
  (bind query "SELECT 1")
  (sqlite_query))
