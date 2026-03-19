;; Check if a URL is healthy and responsive
(define (health-check (url : String))
  (bind url "http://localhost:8080")
  (http_health_check))
