;; Download a file from a URL via HTTP
(define (http-download (url : String) (output_path : String))
  (bind url "https://example.com/file")
  (bind output_path "/tmp/download")
  (http_download))
