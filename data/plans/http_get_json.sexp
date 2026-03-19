;; HTTP get json from endpoint
(define (http-get-json (url : String))
  (bind url "http://localhost:8080")
  (http_get))
