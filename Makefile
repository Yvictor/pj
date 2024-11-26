image:
	docker build --rm -t sinotrade/pj:latest .

run:
	docker run -it --rm sinotrade/pj:latest

help:
	docker run -it --rm sinotrade/pj:latest pj --help

version:
	docker run -it --rm sinotrade/pj:latest pj -V

push:
	docker push sinotrade/pj:latest
