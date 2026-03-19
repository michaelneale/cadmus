;; Read a range of lines from a file
(define (read-file-lines (file : String) (start : String) (end_line : String))
  (bind file "src/main.rs")
  (bind start "1")
  (bind end_line "50")
  (read_lines))
