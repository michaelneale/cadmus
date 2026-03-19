;; Install dependencies then build
(define (install-and-build (dir : String))
  (bind dir ".")
  (install_deps)
  (build_project :dir "$dir"))
