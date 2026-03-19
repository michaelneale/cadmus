;; Clone a GitHub repo and build it
(define (clone-and-build (repo : String) (dir : String))
  (bind repo "owner/repo")
  (bind dir "/tmp/clone")
  (gh_repo_clone)
  (build_project :dir "$dir"))
