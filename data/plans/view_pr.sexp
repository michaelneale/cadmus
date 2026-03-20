;; View pr details
(define (view-pr (dir : String) (pr_number : String))
  (bind dir ".")
  (bind pr_number "1")
  (gh_pr_view))
