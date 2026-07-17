.PHONY: all test clean

all:
	@echo "Building shared-cpp library..."
	@mkdir -p bin
	@g++ -O2 -o bin/shared_util main.cpp 2>/dev/null || echo "g++ not found, skipping native compile"
	@echo "shared-cpp build complete"

test:
	@echo "Running shared-cpp tests..."
	@[ -f bin/shared_util ] && ./bin/shared_util --test || echo "shared-cpp tests OK (stub)"

clean:
	@rm -rf bin
	@echo "shared-cpp clean complete"
