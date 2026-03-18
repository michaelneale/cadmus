;; Find where a function or type is defined in a codebase
(define (find-definition (dir : String) (name : String))
  (bind dir ".")
  (bind name "main")
  (find_definition))
