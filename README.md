# gmail_fetcher
Rust app which queries my gmail for new emails and saves attachments 


Just build the Dockerfile:
``docker build -t gmail_fetcher_app .``

and run it (it will keep running, polling gmail every minute):
``docker run -d --name gmail_fetcher_service gmail_fetcher_app``


