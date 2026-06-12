#!/usr/bin/env python3
"""Tool from https://www.drive.com.au/news/mercedes-benz-commits-to-bringing-back-phycial-buttons/: https://www.drive.com.au/news/mercedes-benz-commits-to-bringing-back-phycial-buttons/"""
import json

def analyze():
    return {"paper": "https://www.drive.com.au/news/mercedes-benz-commits-to-bringing-back-phycial-buttons/", "title": "https://www.drive.com.au/news/mercedes-benz-commits-to-bringing-back-phycial-buttons/", "status": "ready"}

if __name__ == "__main__":
    print(json.dumps(analyze(), indent=2))