;; SSH connect to remote host
(define (ssh (host : String) (command : String))
  (bind host "user@host")
  (bind command "hostname")
  (ssh_run))
