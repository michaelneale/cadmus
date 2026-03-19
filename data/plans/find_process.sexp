;; Find running processes matching a pattern
(define (find-process (pattern : String))
  (bind pattern "node")
  (ps_grep))
