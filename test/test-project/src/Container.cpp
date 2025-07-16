#include "Container.hpp"
#include <iostream>
#include <string>
#include <random>
#include <chrono>

namespace TestProject {

// Template specialization implementations and explicit instantiations

// Explicit instantiation for common types
template class Container<int>;
template class Container<double>;
template class Container<std::string>;
template class Container<float>;
template class Container<char>;

// Explicit instantiation for bool specialization
template class Container<bool>;

// Helper function for demonstration
template<typename T>
void demonstrate_container_operations(const std::string& type_name) {
    std::cout << "=== Container<" << type_name << "> Operations ===" << std::endl;
    
    Container<T> container;
    
    // Add some elements based on type
    if constexpr (std::is_same_v<T, int>) {
        container.push_back(static_cast<T>(1));
        container.push_back(static_cast<T>(2));
        container.push_back(static_cast<T>(3));
        container.push_back(static_cast<T>(4));
        container.push_back(static_cast<T>(5));
    } else if constexpr (std::is_same_v<T, double>) {
        container.push_back(static_cast<T>(1.5));
        container.push_back(static_cast<T>(2.7));
        container.push_back(static_cast<T>(3.14));
        container.push_back(static_cast<T>(4.0));
        container.push_back(static_cast<T>(5.5));
    } else if constexpr (std::is_same_v<T, std::string>) {
        container.push_back("Hello");
        container.push_back("World");
        container.push_back("Template");
        container.push_back("Container");
        container.push_back("Test");
    }
    
    std::cout << "Container size: " << container.size() << std::endl;
    std::cout << "Container empty: " << (container.empty() ? "true" : "false") << std::endl;
    std::cout << "Container capacity: " << container.capacity() << std::endl;
    
    // Test iterator operations
    std::cout << "Elements: ";
    for (const auto& elem : container) {
        std::cout << elem << " ";
    }
    std::cout << std::endl;
    
    // Test statistical operations for numeric types
    if constexpr (std::is_arithmetic_v<T>) {
        try {
            auto stats = container.compute_statistics();
            std::cout << "Statistics - Count: " << stats.count 
                      << ", Min: " << stats.min_value 
                      << ", Max: " << stats.max_value << std::endl;
        } catch (const std::exception& e) {
            std::cout << "Statistics error: " << e.what() << std::endl;
        }
    }
    
    // Test transform operation
    auto transformed = container.transform([](const T& val) {
        if constexpr (std::is_arithmetic_v<T>) {
            return val * static_cast<T>(2);
        } else {
            return val; // For non-arithmetic types, return as-is
        }
    });
    
    std::cout << "Transformed elements: ";
    for (const auto& elem : transformed) {
        std::cout << elem << " ";
    }
    std::cout << std::endl;
    
    // Test sorting for comparable types
    if constexpr (std::is_arithmetic_v<T> || std::is_same_v<T, std::string>) {
        container.sort();
        std::cout << "Sorted elements: ";
        for (const auto& elem : container) {
            std::cout << elem << " ";
        }
        std::cout << std::endl;
    }
    
    // Test advanced operations
    if constexpr (std::is_arithmetic_v<T>) {
        auto count_positive = container.count_if([](const T& val) { return val > T{0}; });
        std::cout << "Positive elements count: " << count_positive << std::endl;
        
        auto found = container.find_if([](const T& val) { return val > T{2}; });
        if (found != container.end()) {
            std::cout << "First element > 2: " << *found << std::endl;
        }
        
        bool all_positive = container.all_of([](const T& val) { return val > T{0}; });
        std::cout << "All elements positive: " << (all_positive ? "true" : "false") << std::endl;
    }
    
    std::cout << std::endl;
}

// Explicit instantiation of demonstration function
template void demonstrate_container_operations<int>(const std::string&);
template void demonstrate_container_operations<double>(const std::string&);
template void demonstrate_container_operations<std::string>(const std::string&);

// Template function for creating containers with random data
template<typename T>
Container<T> create_random_container(size_t size, T min_val, T max_val) {
    Container<T> container;
    container.reserve(size);
    
    std::random_device rd;
    std::mt19937 gen(rd());
    
    if constexpr (std::is_integral_v<T>) {
        std::uniform_int_distribution<T> dis(min_val, max_val);
        for (size_t i = 0; i < size; ++i) {
            container.push_back(dis(gen));
        }
    } else if constexpr (std::is_floating_point_v<T>) {
        std::uniform_real_distribution<T> dis(min_val, max_val);
        for (size_t i = 0; i < size; ++i) {
            container.push_back(dis(gen));
        }
    }
    
    return container;
}

// Explicit instantiation of random container function
template Container<int> create_random_container<int>(size_t, int, int);
template Container<double> create_random_container<double>(size_t, double, double);
template Container<float> create_random_container<float>(size_t, float, float);

// Template function for merging two containers
template<typename T>
Container<T> merge_containers(const Container<T>& first, const Container<T>& second) {
    Container<T> result;
    result.reserve(first.size() + second.size());
    
    // Copy all elements from first container
    for (const auto& elem : first) {
        result.push_back(elem);
    }
    
    // Copy all elements from second container
    for (const auto& elem : second) {
        result.push_back(elem);
    }
    
    return result;
}

// Explicit instantiation of merge function
template Container<int> merge_containers<int>(const Container<int>&, const Container<int>&);
template Container<double> merge_containers<double>(const Container<double>&, const Container<double>&);
template Container<std::string> merge_containers<std::string>(const Container<std::string>&, const Container<std::string>&);

// Template function for filtering containers
template<typename T, typename Predicate>
Container<T> filter_container(const Container<T>& source, Predicate pred) {
    Container<T> result;
    
    for (const auto& elem : source) {
        if (pred(elem)) {
            result.push_back(elem);
        }
    }
    
    return result;
}

// Explicit instantiation of filter function for common predicates
template Container<int> filter_container<int>(const Container<int>&, std::function<bool(const int&)>);
template Container<double> filter_container<double>(const Container<double>&, std::function<bool(const double&)>);

// Template function for parallel-style operations (conceptual)
template<typename T>
void parallel_process_container(Container<T>& container) {
    // Simulate parallel processing
    std::for_each(container.begin(), container.end(), [](T& elem) {
        if constexpr (std::is_arithmetic_v<T>) {
            elem = elem * static_cast<T>(2);  // Double each element
        }
    });
}

// Explicit instantiation of parallel function
template void parallel_process_container<int>(Container<int>&);
template void parallel_process_container<double>(Container<double>&);

// Template function for container benchmarking
template<typename T>
void benchmark_container_operations(size_t iterations) {
    std::cout << "=== Benchmarking Container Operations ===" << std::endl;
    
    Container<T> container;
    
    // Benchmark push_back operations
    auto start = std::chrono::high_resolution_clock::now();
    for (size_t i = 0; i < iterations; ++i) {
        if constexpr (std::is_arithmetic_v<T>) {
            container.push_back(static_cast<T>(i));
        } else if constexpr (std::is_same_v<T, std::string>) {
            container.push_back("item" + std::to_string(i));
        }
    }
    auto end = std::chrono::high_resolution_clock::now();
    
    auto duration = std::chrono::duration_cast<std::chrono::microseconds>(end - start);
    std::cout << "Push operations: " << duration.count() << " microseconds" << std::endl;
    
    // Benchmark random access
    start = std::chrono::high_resolution_clock::now();
    for (size_t i = 0; i < iterations; ++i) {
        volatile auto val = container[i % container.size()];
        (void)val; // Prevent optimization
    }
    end = std::chrono::high_resolution_clock::now();
    
    duration = std::chrono::duration_cast<std::chrono::microseconds>(end - start);
    std::cout << "Random access: " << duration.count() << " microseconds" << std::endl;
    
    // Benchmark iteration
    start = std::chrono::high_resolution_clock::now();
    for (size_t i = 0; i < 100; ++i) {  // Fewer iterations for this operation
        for (const auto& elem : container) {
            volatile auto val = elem;
            (void)val; // Prevent optimization
        }
    }
    end = std::chrono::high_resolution_clock::now();
    
    duration = std::chrono::duration_cast<std::chrono::microseconds>(end - start);
    std::cout << "Iteration: " << duration.count() << " microseconds" << std::endl;
    
    std::cout << std::endl;
}

// Explicit instantiation of benchmark function
template void benchmark_container_operations<int>(size_t);
template void benchmark_container_operations<double>(size_t);

} // namespace TestProject