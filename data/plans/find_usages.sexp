;; Find all usages of a symbol across a codebase
(define (find-usages (dir : String) (symbol : String))
  (bind dir ".")
  (bind symbol "main")
  (find_usages))
