;; Run a command on a remote host via SSH
(define (ssh-command (host : String) (command : String))
  (bind host "user@host")
  (bind command "hostname")
  (ssh_run))
