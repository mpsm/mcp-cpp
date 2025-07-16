#pragma once

#include <vector>
#include <cmath>
#include <array>
#include <complex>
#include <functional>
#include <limits>
#include <type_traits>
#include <numeric>
#include <algorithm>

namespace TestProject {

/**
 * @brief Enhanced mathematical utility class with various calculation methods
 */
class Math {
public:
    // Nested class for statistical operations
    class Statistics {
    public:
        struct Result {
            double mean;
            double variance;
            double standard_deviation;
            double median;
            double min;
            double max;
            size_t count;
            
            Result() : mean(0), variance(0), standard_deviation(0), median(0), 
                      min(0), max(0), count(0) {}
        };
        
        // Nested class for distribution analysis
        class Distribution {
        public:
            enum Type {
                NORMAL,
                UNIFORM,
                EXPONENTIAL,
                UNKNOWN
            };
            
            Type type;
            double parameter1;
            double parameter2;
            double confidence;
            
            Distribution(Type t = UNKNOWN, double p1 = 0, double p2 = 0, double conf = 0)
                : type(t), parameter1(p1), parameter2(p2), confidence(conf) {}
            
            std::string to_string() const;
        };
        
        static Result analyze(const std::vector<double>& values);
        static Distribution detect_distribution(const std::vector<double>& values);
        static double correlation(const std::vector<double>& x, const std::vector<double>& y);
        static std::vector<double> percentiles(const std::vector<double>& values, const std::vector<double>& percentile_points);
    };
    
    // Nested class for complex number operations
    class Complex {
    public:
        using complex_t = std::complex<double>;
        
        static complex_t add(const complex_t& a, const complex_t& b);
        static complex_t multiply(const complex_t& a, const complex_t& b);
        static complex_t divide(const complex_t& a, const complex_t& b);
        static complex_t power(const complex_t& base, const complex_t& exponent);
        static complex_t sqrt(const complex_t& value);
        static complex_t exp(const complex_t& value);
        static complex_t log(const complex_t& value);
        
        // Polar form conversions
        static complex_t from_polar(double magnitude, double angle);
        static std::pair<double, double> to_polar(const complex_t& value);
        
        // Complex roots
        static std::vector<complex_t> roots(const complex_t& value, int n);
    };
    
    // Nested class for matrix operations
    template<typename T, size_t Rows, size_t Cols>
    class Matrix {
    private:
        std::array<std::array<T, Cols>, Rows> data_;
        
    public:
        Matrix() { fill(T{}); }
        
        Matrix(const std::array<std::array<T, Cols>, Rows>& data) : data_(data) {}
        
        // Element access
        T& operator()(size_t row, size_t col) { return data_[row][col]; }
        const T& operator()(size_t row, size_t col) const { return data_[row][col]; }
        
        // Matrix operations
        Matrix operator+(const Matrix& other) const;
        Matrix operator-(const Matrix& other) const;
        Matrix operator*(const Matrix& other) const requires (Cols == Rows);
        Matrix operator*(T scalar) const;
        
        // Matrix properties
        T determinant() const requires (Rows == Cols);
        Matrix transpose() const;
        Matrix inverse() const requires (Rows == Cols);
        T trace() const requires (Rows == Cols);
        
        // Utility methods
        void fill(T value);
        static Matrix identity() requires (Rows == Cols);
        static Matrix zero();
        
        // Nested iterator class
        class Iterator {
        private:
            Matrix* matrix_;
            size_t row_, col_;
            
        public:
            Iterator(Matrix* matrix, size_t row, size_t col) 
                : matrix_(matrix), row_(row), col_(col) {}
            
            T& operator*() { return (*matrix_)(row_, col_); }
            Iterator& operator++();
            bool operator!=(const Iterator& other) const;
            bool operator==(const Iterator& other) const;
        };
        
        Iterator begin() { return Iterator(this, 0, 0); }
        Iterator end() { return Iterator(this, Rows, 0); }
        
        constexpr size_t rows() const { return Rows; }
        constexpr size_t cols() const { return Cols; }
    };
    
    // Type aliases for common matrix types
    using Matrix2x2 = Matrix<double, 2, 2>;
    using Matrix3x3 = Matrix<double, 3, 3>;
    using Matrix4x4 = Matrix<double, 4, 4>;
    using IntMatrix2x2 = Matrix<int, 2, 2>;
    using IntMatrix3x3 = Matrix<int, 3, 3>;

    // Original methods with overloads
    /**
     * @brief Calculate the factorial of a number
     * @param n The number to calculate factorial for
     * @return The factorial of n
     */
    static long long factorial(int n);
    static long long factorial(unsigned int n);
    static double factorial(double n);  // Gamma function approximation
    
    /**
     * @brief Calculate the greatest common divisor of two numbers
     * @param a First number
     * @param b Second number
     * @return The GCD of a and b
     */
    static int gcd(int a, int b);
    static long long gcd(long long a, long long b);
    static unsigned int gcd(unsigned int a, unsigned int b);
    
    // Template version for any integer type
    template<typename T>
    static T gcd(T a, T b) requires std::is_integral_v<T>;
    
    /**
     * @brief Calculate the mean of a vector of numbers
     * @param values Vector of numbers
     * @return The mean value
     */
    static double mean(const std::vector<double>& values);
    static float mean(const std::vector<float>& values);
    static double mean(const std::vector<int>& values);
    static double mean(const std::vector<long long>& values);
    
    // Template version for any numeric type
    template<typename T>
    static double mean(const std::vector<T>& values) requires std::is_arithmetic_v<T>;
    
    // Array overloads
    template<typename T, size_t N>
    static double mean(const std::array<T, N>& values) requires std::is_arithmetic_v<T>;
    
    // C-style array overloads
    template<typename T>
    static double mean(const T* values, size_t count) requires std::is_arithmetic_v<T>;
    
    /**
     * @brief Calculate the standard deviation of a vector of numbers
     * @param values Vector of numbers
     * @return The standard deviation
     */
    static double standardDeviation(const std::vector<double>& values);
    static float standardDeviation(const std::vector<float>& values);
    static double standardDeviation(const std::vector<int>& values);
    
    // Template version
    template<typename T>
    static double standardDeviation(const std::vector<T>& values) requires std::is_arithmetic_v<T>;
    
    /**
     * @brief Check if a number is prime
     * @param n Number to check
     * @return true if prime, false otherwise
     */
    static bool isPrime(int n);
    static bool isPrime(long long n);
    static bool isPrime(unsigned int n);
    static bool isPrime(unsigned long long n);
    
    // Template version
    template<typename T>
    static bool isPrime(T n) requires std::is_integral_v<T>;
    
    // Advanced mathematical functions
    /**
     * @brief Calculate power with various overloads
     */
    static double power(double base, double exponent);
    static float power(float base, float exponent);
    static int power(int base, int exponent);
    static long long power(long long base, long long exponent);
    
    template<typename T>
    static T power(T base, T exponent) requires std::is_arithmetic_v<T>;
    
    /**
     * @brief Calculate logarithm with various bases
     */
    static double log(double value);
    static double log(double value, double base);
    static float log(float value);
    static float log(float value, float base);
    
    template<typename T>
    static double log(T value) requires std::is_arithmetic_v<T>;
    
    template<typename T>
    static double log(T value, T base) requires std::is_arithmetic_v<T>;
    
    /**
     * @brief Calculate trigonometric functions
     */
    static double sin(double angle);
    static double cos(double angle);
    static double tan(double angle);
    static double asin(double value);
    static double acos(double value);
    static double atan(double value);
    static double atan2(double y, double x);
    
    // Overloads for float
    static float sin(float angle);
    static float cos(float angle);
    static float tan(float angle);
    static float asin(float value);
    static float acos(float value);
    static float atan(float value);
    static float atan2(float y, float x);
    
    /**
     * @brief Calculate hyperbolic functions
     */
    static double sinh(double value);
    static double cosh(double value);
    static double tanh(double value);
    static double asinh(double value);
    static double acosh(double value);
    static double atanh(double value);
    
    // Float overloads
    static float sinh(float value);
    static float cosh(float value);
    static float tanh(float value);
    static float asinh(float value);
    static float acosh(float value);
    static float atanh(float value);
    
    /**
     * @brief Calculate least common multiple
     */
    static int lcm(int a, int b);
    static long long lcm(long long a, long long b);
    static unsigned int lcm(unsigned int a, unsigned int b);
    
    template<typename T>
    static T lcm(T a, T b) requires std::is_integral_v<T>;
    
    /**
     * @brief Calculate absolute value
     */
    static int abs(int value);
    static long long abs(long long value);
    static float abs(float value);
    static double abs(double value);
    
    template<typename T>
    static T abs(T value) requires std::is_arithmetic_v<T>;
    
    /**
     * @brief Calculate square root
     */
    static double sqrt(double value);
    static float sqrt(float value);
    
    template<typename T>
    static double sqrt(T value) requires std::is_arithmetic_v<T>;
    
    /**
     * @brief Calculate nth root
     */
    static double nthRoot(double value, int n);
    static float nthRoot(float value, int n);
    
    template<typename T>
    static double nthRoot(T value, int n) requires std::is_arithmetic_v<T>;
    
    /**
     * @brief Rounding functions
     */
    static int round(double value);
    static int round(float value);
    static long long round(double value, int precision);
    static int floor(double value);
    static int floor(float value);
    static int ceil(double value);
    static int ceil(float value);
    
    template<typename T>
    static int round(T value) requires std::is_floating_point_v<T>;
    
    /**
     * @brief Min/Max functions with multiple overloads
     */
    static int min(int a, int b);
    static double min(double a, double b);
    static float min(float a, float b);
    static long long min(long long a, long long b);
    
    template<typename T>
    static T min(T a, T b) requires std::is_arithmetic_v<T>;
    
    template<typename T>
    static T min(std::initializer_list<T> values) requires std::is_arithmetic_v<T>;
    
    template<typename T>
    static T min(const std::vector<T>& values) requires std::is_arithmetic_v<T>;
    
    static int max(int a, int b);
    static double max(double a, double b);
    static float max(float a, float b);
    static long long max(long long a, long long b);
    
    template<typename T>
    static T max(T a, T b) requires std::is_arithmetic_v<T>;
    
    template<typename T>
    static T max(std::initializer_list<T> values) requires std::is_arithmetic_v<T>;
    
    template<typename T>
    static T max(const std::vector<T>& values) requires std::is_arithmetic_v<T>;
    
    /**
     * @brief Clamp function
     */
    template<typename T>
    static T clamp(T value, T min_val, T max_val) requires std::is_arithmetic_v<T>;
    
    /**
     * @brief Linear interpolation
     */
    static double lerp(double a, double b, double t);
    static float lerp(float a, float b, float t);
    
    template<typename T>
    static T lerp(T a, T b, T t) requires std::is_arithmetic_v<T>;
    
    /**
     * @brief Numerical integration
     */
    static double integrate(std::function<double(double)> f, double a, double b, int n = 1000);
    static double trapezoidalRule(std::function<double(double)> f, double a, double b, int n);
    static double simpsonsRule(std::function<double(double)> f, double a, double b, int n);
    
    /**
     * @brief Numerical differentiation
     */
    static double derivative(std::function<double(double)> f, double x, double h = 1e-8);
    static double secondDerivative(std::function<double(double)> f, double x, double h = 1e-8);
    
    /**
     * @brief Polynomial operations
     */
    static double evaluatePolynomial(const std::vector<double>& coefficients, double x);
    static std::vector<double> multiplyPolynomials(const std::vector<double>& a, const std::vector<double>& b);
    static std::vector<double> addPolynomials(const std::vector<double>& a, const std::vector<double>& b);
    static std::vector<double> subtractPolynomials(const std::vector<double>& a, const std::vector<double>& b);
    
    /**
     * @brief Constants
     */
    static constexpr double PI = 3.141592653589793;
    static constexpr double E = 2.718281828459045;
    static constexpr double GOLDEN_RATIO = 1.618033988749895;
    static constexpr double SQRT_2 = 1.414213562373095;
    static constexpr double SQRT_3 = 1.732050807568877;
    static constexpr double LN_2 = 0.693147180559945;
    static constexpr double LN_10 = 2.302585092994046;
    
    /**
     * @brief Utility functions
     */
    template<typename T>
    static bool isNaN(T value) requires std::is_floating_point_v<T>;
    
    template<typename T>
    static bool isInfinite(T value) requires std::is_floating_point_v<T>;
    
    template<typename T>
    static bool isFinite(T value) requires std::is_floating_point_v<T>;
    
    template<typename T>
    static bool isEqual(T a, T b, T epsilon = std::numeric_limits<T>::epsilon()) requires std::is_floating_point_v<T>;

private:
    // Private helper methods
    static double variance(const std::vector<double>& values);
    static double variance(const std::vector<float>& values);
    static double variance(const std::vector<int>& values);
    
    template<typename T>
    static double variance(const std::vector<T>& values) requires std::is_arithmetic_v<T>;
    
    // Helper for prime checking
    template<typename T>
    static bool isPrimeHelper(T n) requires std::is_integral_v<T>;
    
    // Helper for power calculation
    template<typename T>
    static T powerHelper(T base, T exponent) requires std::is_arithmetic_v<T>;
    
    // Helper for numerical integration
    static double integrateHelper(std::function<double(double)> f, double a, double b, int n, 
                                 std::function<double(std::function<double(double)>, double, double, int)> method);
};

// Template function implementations
template<typename T, size_t N>
double Math::mean(const std::array<T, N>& values) requires std::is_arithmetic_v<T> {
    if (values.empty()) {
        throw std::invalid_argument("Cannot calculate mean of empty array");
    }
    double sum = std::accumulate(values.begin(), values.end(), 0.0);
    return sum / values.size();
}

template<typename T>
T Math::min(std::initializer_list<T> values) requires std::is_arithmetic_v<T> {
    if (values.size() == 0) {
        throw std::invalid_argument("Cannot find min of empty initializer list");
    }
    return *std::min_element(values.begin(), values.end());
}

template<typename T>
T Math::max(const std::vector<T>& values) requires std::is_arithmetic_v<T> {
    if (values.empty()) {
        throw std::invalid_argument("Cannot find max of empty vector");
    }
    return *std::max_element(values.begin(), values.end());
}

// Matrix template implementations
template<typename T, size_t Rows, size_t Cols>
Math::Matrix<T, Rows, Cols> Math::Matrix<T, Rows, Cols>::operator+(const Matrix& other) const {
    Matrix result;
    for (size_t i = 0; i < Rows; ++i) {
        for (size_t j = 0; j < Cols; ++j) {
            result(i, j) = (*this)(i, j) + other(i, j);
        }
    }
    return result;
}

template<typename T, size_t Rows, size_t Cols>
void Math::Matrix<T, Rows, Cols>::fill(T value) {
    for (size_t i = 0; i < Rows; ++i) {
        for (size_t j = 0; j < Cols; ++j) {
            data_[i][j] = value;
        }
    }
}

} // namespace TestProject