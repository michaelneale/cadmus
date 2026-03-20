;; Fix ci: view failed run then rerun
(define (fix-ci (dir : String) (run_id : String))
  (bind dir ".")
  (bind run_id "1")
  (gh_run_view)
  (gh_run_rerun :dir "$dir" :run_id "$run_id"))
