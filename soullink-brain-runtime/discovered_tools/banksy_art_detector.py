#!/usr/bin/env python3
"""Tool from https://www.smithsonianmag.com/smart-news/attributed-to-banksy-a-new-statue-of-a-suited-man-blinded-by-a-flag-and-walking-off-a-ledge-appeared-in-central-london-180988662/: https://www.smithsonianmag.com/smart-news/attributed-to-banksy-a-new-statue-of-a-suited-man-blinded-by-a-flag-and-walking-off-a-ledge-appeared-in-central-london-180988662/"""
import json

def analyze():
    return {"paper": "https://www.smithsonianmag.com/smart-news/attributed-to-banksy-a-new-statue-of-a-suited-man-blinded-by-a-flag-and-walking-off-a-ledge-appeared-in-central-london-180988662/", "title": "https://www.smithsonianmag.com/smart-news/attributed-to-banksy-a-new-statue-of-a-suited-man-blinded-by-a-flag-and-walking-off-a-ledge-appeared-in-central-london-180988662/", "status": "ready"}

if __name__ == "__main__":
    print(json.dumps(analyze(), indent=2))