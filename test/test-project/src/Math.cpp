#include "Math.hpp"
#include <stdexcept>
#include <numeric>
#include <algorithm>

namespace TestProject {

long long Math::factorial(int n) {
    if (n < 0) {
        throw std::invalid_argument("Factorial is not defined for negative numbers");
    }
    
    if (n == 0 || n == 1) {
        return 1;
    }
    
    long long result = 1;
    for (int i = 2; i <= n; ++i) {
        result *= i;
    }
    return result;
}

int Math::gcd(int a, int b) {
    a = std::abs(a);
    b = std::abs(b);
    
    while (b != 0) {
        int temp = b;
        b = a % b;
        a = temp;
    }
    return a;
}

double Math::mean(const std::vector<double>& values) {
    if (values.empty()) {
        throw std::invalid_argument("Cannot calculate mean of empty vector");
    }
    
    double sum = std::accumulate(values.begin(), values.end(), 0.0);
    return sum / values.size();
}

double Math::variance(const std::vector<double>& values) {
    if (values.empty()) {
        return 0.0;
    }
    
    double avg = mean(values);
    double sum_squared_diff = 0.0;
    
    for (const auto& value : values) {
        double diff = value - avg;
        sum_squared_diff += diff * diff;
    }
    
    return sum_squared_diff / values.size();
}

double Math::standardDeviation(const std::vector<double>& values) {
    return std::sqrt(variance(values));
}

bool Math::isPrime(int n) {
    if (n <= 1) {
        return false;
    }
    
    if (n <= 3) {
        return true;
    }
    
    if (n % 2 == 0 || n % 3 == 0) {
        return false;
    }
    
    for (int i = 5; i * i <= n; i += 6) {
        if (n % i == 0 || n % (i + 2) == 0) {
            return false;
        }
    }
    
    return true;
}

} // namespace TestProject