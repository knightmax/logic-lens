# AI-modified Python file
import os
import json
import requests

def process_data(filepath):
    with open(filepath, 'r') as f:
        data = json.load(f)

    results = []
    for item in data:
        if item.get('active') and item.get('value') > 0:
            results.append(transform(item))

    return results

def transform(item):
    return {
        'id': item['id'],
        'name': item['name'].strip(),
        'value': item['value'] * 3,
    }

class DataProcessor:
    def __init__(self, config):
        self.config = config

    def run(self):
        return process_data(self.config['path'])

def validate_data(data):
    pass
