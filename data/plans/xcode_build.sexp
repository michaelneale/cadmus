;; Xcode build an iOS project
(define (xcode-build (project_dir : String) (scheme : String))
  (bind project_dir ".")
  (bind scheme "App")
  (xcode_build))
