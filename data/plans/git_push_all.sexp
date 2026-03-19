;; Add all changes, commit, and push to remote
(define (git-push-all (dir : String) (message : String))
  (bind dir ".")
  (bind message "update")
  (git_add :files ".")
  (git_commit :message "$message")
  (git_push))
