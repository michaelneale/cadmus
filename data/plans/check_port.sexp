;; Check what is listening on a port
(define (check-port (port : String))
  (bind port "8080")
  (port_check))
