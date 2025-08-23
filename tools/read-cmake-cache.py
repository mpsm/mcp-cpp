#!/usr/bin/env python3
"""
Simple ccmake-like cache viewer - shows only user-configurable, non-advanced entries
"""

import sys
import os

def parse_cmake_cache(cache_file):
    """Parse CMakeCache.txt and return external entries and advanced properties"""
    external_entries = {}
    advanced_entries = set()
    
    try:
        with open(cache_file, 'r') as f:
            in_external = False
            in_internal = False
            
            for line in f:
                line = line.strip()
                
                # Section markers
                if "# EXTERNAL cache entries" in line:
                    in_external = True
                    continue
                elif "# INTERNAL cache entries" in line:
                    in_external = False
                    in_internal = True
                    continue
                
                # Skip comments and empty lines
                if not line or line.startswith('//') or line.startswith('#'):
                    continue
                
                # Parse cache entries: KEY:TYPE=VALUE
                if ':' in line and '=' in line:
                    colon_pos = line.find(':')
                    equals_pos = line.find('=')
                    if colon_pos < equals_pos:
                        key = line[:colon_pos]
                        type_part = line[colon_pos+1:equals_pos]
                        value = line[equals_pos+1:]
                        
                        if in_external:
                            # Store external entries
                            external_entries[key] = {
                                'type': type_part,
                                'value': value
                            }
                        elif in_internal and key.endswith('-ADVANCED'):
                            # Track advanced properties
                            if value == '1':
                                original_key = key[:-9]  # Remove '-ADVANCED'
                                advanced_entries.add(original_key)
    
    except FileNotFoundError:
        print(f"Error: CMakeCache.txt not found at {cache_file}")
        sys.exit(1)
    
    return external_entries, advanced_entries

def is_user_configurable(key, entry_type):
    """Determine if entry should be shown to user"""
    # Filter out computed/read-only entries
    if entry_type == 'STATIC':
        return False
    if entry_type == 'INTERNAL':
        return False
    
    # Include common user-configurable types
    user_types = {'STRING', 'BOOL', 'PATH', 'FILEPATH'}
    return entry_type in user_types

def display_cache_entries(cache_file):
    """Display non-advanced, user-configurable cache entries"""
    external_entries, advanced_entries = parse_cmake_cache(cache_file)
    
    # Filter and display entries
    shown_entries = []
    
    for key, entry in external_entries.items():
        if key not in advanced_entries and is_user_configurable(key, entry['type']):
            shown_entries.append((key, entry['value']))
    
    # Display results
    if not shown_entries:
        print("No user-configurable entries found")
        return
    
    print("Non-advanced cache entries:")
    print("-" * 40)
    
    # Find max key length for alignment
    max_key_len = max(len(key) for key, _ in shown_entries)
    
    for key, value in sorted(shown_entries):
        # Display format similar to ccmake
        if value:
            print(f" {key:<{max_key_len}} {value}")
        else:
            print(f" {key}")

if __name__ == "__main__":
    # Default to current directory's CMakeCache.txt
    cache_file = "CMakeCache.txt"
    
    if len(sys.argv) > 1:
        cache_file = sys.argv[1]
    
    if not os.path.exists(cache_file):
        print(f"Usage: {sys.argv[0]} [path/to/CMakeCache.txt]")
        print(f"Default: looks for CMakeCache.txt in current directory")
        sys.exit(1)
    
    display_cache_entries(cache_file)