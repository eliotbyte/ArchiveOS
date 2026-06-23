from __future__ import annotations

from unittest.mock import Mock

import pytest
import requests

from worker.core_client import (
    api_error_message,
    http_error_message,
    manifest_submit_retryable,
)
from worker.failures import classify_import_error


def test_api_error_message_reads_json_error_field():
    response = Mock(spec=requests.Response)
    response.json.return_value = {"error": "import file not found (managed): /vaults/x/staging/a/files/v.mp4"}
    response.text = ""
    response.reason = "Bad Request"
    response.status_code = 400

    assert "import file not found" in api_error_message(response)


def test_http_error_message_includes_body():
    response = Mock(spec=requests.Response)
    response.json.return_value = {"error": "manifest missing source_identity for collection import"}
    response.text = ""
    response.reason = "Bad Request"
    response.status_code = 400

    message = http_error_message(response)
    assert "400 Bad Request" in message
    assert "manifest missing source_identity" in message


@pytest.mark.parametrize(
    ("status_code", "body", "expected"),
    [
        (400, "import file not found (managed): /tmp/x", True),
        (400, "manifest missing source_identity", False),
        (500, "database locked", True),
        (503, "vault unavailable", True),
    ],
)
def test_manifest_submit_retryable(status_code: int, body: str, expected: bool):
    response = Mock(spec=requests.Response)
    response.status_code = status_code
    assert manifest_submit_retryable(body, response) is expected


def test_classify_import_error_staging_missing_is_retryable():
    kind, retryable = classify_import_error(
        "400 Bad Request: import file not found (managed): /vaults/archiveos/staging/job/files/v.mp4"
    )
    assert kind == "staging_file_missing"
    assert retryable is True


def test_classify_import_error_rejected_not_retryable():
    kind, retryable = classify_import_error(
        "400 Bad Request: manifest missing source_identity for collection import"
    )
    assert kind == "import_rejected"
    assert retryable is False
