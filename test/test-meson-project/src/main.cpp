#include <iostream>
#include "math.h"
#include "utils.h"

int main() {
    std::cout << "Meson Test Project" << std::endl;
    std::cout << "5! = " << factorial(5) << std::endl;
    std::cout << "gcd(48, 18) = " << gcd(48, 18) << std::endl;
    print_message("Hello from Meson!");
    return 0;
}