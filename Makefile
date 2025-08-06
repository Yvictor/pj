# Extract version from Cargo.toml
VERSION := $(shell grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
IMAGE_NAME := sinotrade/pj

.PHONY: all image run help version push push-version push-latest clean-images show-version show-tags tag-version list-images

# Default target
all: help

# Help message
help:
	@echo "PJ Docker Makefile - Version $(VERSION)"
	@echo ""
	@echo "Available targets:"
	@echo "  make image          - Remove existing tags and build new images ($(VERSION) & latest)"
	@echo "  make image-keep     - Build without removing existing images"
	@echo "  make image-nocache  - Build with --no-cache flag"
	@echo "  make run            - Run the latest image interactively"
	@echo "  make run-help       - Show pj help message"
	@echo "  make run-version    - Show pj version"
	@echo "  make push           - Push both version and latest tags to registry"
	@echo "  make push-version   - Push only version tag ($(VERSION))"
	@echo "  make push-latest    - Push only latest tag"
	@echo "  make clean-images   - Remove existing Docker images"
	@echo "  make show-version   - Display current version from Cargo.toml"
	@echo "  make show-tags      - Show all tags that will be created"
	@echo "  make list-images    - List all local pj Docker images"
	@echo "  make tag-version    - Tag existing latest as version $(VERSION)"
	@echo ""
	@echo "Current tags:"
	@echo "  - $(IMAGE_NAME):$(VERSION)"
	@echo "  - $(IMAGE_NAME):latest"

image: clean-images
	@echo "Building Docker images with version $(VERSION)..."
	docker build --rm -t $(IMAGE_NAME):$(VERSION) -t $(IMAGE_NAME):latest .
	@echo "Successfully built $(IMAGE_NAME):$(VERSION) and $(IMAGE_NAME):latest"

# Clean existing images before building
clean-images:
	@echo "Cleaning existing Docker images..."
	@docker rmi $(IMAGE_NAME):$(VERSION) 2>/dev/null || true
	@docker rmi $(IMAGE_NAME):latest 2>/dev/null || true
	@echo "Cleaned existing images"

# Build without cleaning (useful for quick rebuilds)
image-nocache:
	@echo "Building Docker images with version $(VERSION) (no cache)..."
	docker build --no-cache --rm -t $(IMAGE_NAME):$(VERSION) -t $(IMAGE_NAME):latest .
	@echo "Successfully built $(IMAGE_NAME):$(VERSION) and $(IMAGE_NAME):latest"

# Build without removing existing images (useful for testing)
image-keep:
	@echo "Building Docker images with version $(VERSION) (keeping existing)..."
	docker build --rm -t $(IMAGE_NAME):$(VERSION) -t $(IMAGE_NAME):latest .
	@echo "Successfully built $(IMAGE_NAME):$(VERSION) and $(IMAGE_NAME):latest"

run:
	docker run -it --rm $(IMAGE_NAME):latest

run-help:
	docker run -it --rm $(IMAGE_NAME):latest pj --help

run-version:
	docker run -it --rm $(IMAGE_NAME):latest pj -V

push: push-version push-latest
	@echo "Successfully pushed all tags"

push-version:
	@echo "Pushing $(IMAGE_NAME):$(VERSION)..."
	docker push $(IMAGE_NAME):$(VERSION)

push-latest:
	@echo "Pushing $(IMAGE_NAME):latest..."
	docker push $(IMAGE_NAME):latest

# Show current version
show-version:
	@echo "Current version: $(VERSION)"

# Show all tags that will be created
show-tags:
	@echo "Docker tags to be created:"
	@echo "  - $(IMAGE_NAME):$(VERSION)"
	@echo "  - $(IMAGE_NAME):latest"

# Tag existing latest image with version (useful if image already built)
tag-version:
	docker tag $(IMAGE_NAME):latest $(IMAGE_NAME):$(VERSION)

# List all local images for this project
list-images:
	@docker images $(IMAGE_NAME) --format "table {{.Tag}}\t{{.Size}}\t{{.CreatedAt}}"
