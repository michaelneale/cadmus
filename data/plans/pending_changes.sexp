;; pending changes
(define (pending-changes (dir : String))
  (bind dir ".")
  (show_changes))
