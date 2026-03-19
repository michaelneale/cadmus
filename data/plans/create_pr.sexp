;; Create pr from current branch
(define (create-pr (dir : String) (title : String) (body : String))
  (bind dir ".")
  (bind title "Pull request")
  (bind body "")
  (gh_pr_create))
