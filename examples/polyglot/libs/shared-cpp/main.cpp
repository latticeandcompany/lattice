#include <iostream>
#include <string>

std::string greet(const std::string& name) {
    return "Hello, " + name + "! From the C++ shared library.";
}

int main(int argc, char* argv[]) {
    if (argc > 1 && std::string(argv[1]) == "--test") {
        std::cout << "Running tests..." << std::endl;
        std::cout << greet("Test") << std::endl;
        std::cout << "All tests passed." << std::endl;
        return 0;
    }
    std::cout << greet("World") << std::endl;
    return 0;
}
