;; Count lines in source files
(define (count-lines (path : String))
  (bind path ".")
  (line_count))
