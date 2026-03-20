;; CI runs for the repo
(define (ci-runs (dir : String))
  (bind dir ".")
  (gh_run_list))
