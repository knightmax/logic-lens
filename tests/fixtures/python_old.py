# Original Python file
import os
import json

def process_data(filepath):
    with open(filepath, 'r') as f:
        data = json.load(f)

    results = []
    for item in data:
        if item.get('active'):
            results.append(transform(item))

    return results

def transform(item):
    return {
        'id': item['id'],
        'name': item['name'].strip(),
        'value': item['value'] * 2,
    }

class DataProcessor:
    def __init__(self, config):
        self.config = config

    def run(self):
        return process_data(self.config['path'])
