;; Rerun failed ci jobs
(define (rerun-ci (dir : String) (run_id : String))
  (bind dir ".")
  (bind run_id "1")
  (gh_run_rerun))
