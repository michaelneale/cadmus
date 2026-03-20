;; Blame lines in a file
(define (blame-lines (file : String) (start : String) (end_line : String))
  (bind file "src/main.rs")
  (bind start "1")
  (bind end_line "20")
  (blame_lines))
