;; Build and restart a running process
(define (build-and-restart (dir : String) (process_pattern : String))
  (bind dir ".")
  (bind process_pattern "server")
  (build_and_restart))
