;; View ci run logs
(define (view-ci-run (dir : String) (run_id : String))
  (bind dir ".")
  (bind run_id "1")
  (gh_run_view))
