---
type: api-specification
tags: [api, documentation, endpoints, integration]
created: 2025-01-20
modified: 2025-01-31
status: active
priority: high
aliases: [API Reference, Endpoints, REST API]
related: ["[[Technical Documentation]]", "[[Project Management]]", "[[Contact Management]]"]
api_version: "v2.1.0"
base_url: "https://api.crucible.app"
protocol: "HTTPS"
authentication: "JWT Bearer Token"
rate_limit: "1000 requests/hour"
endpoints_count: 47
category: "technical-specification"
last_updated: "2025-01-31"
---

# API Documentation

Comprehensive REST API specification for the Crucible knowledge management system, including all endpoints, authentication mechanisms, data models, and integration guidelines.

## API Overview

### Base Information
- **Base URL**: `https://api.crucible.app/v2`
- **Protocol**: HTTPS only
- **Content-Type**: `application/json`
- **Character Encoding**: UTF-8
- **API Version**: 2.1.0
- **Documentation Version**: 2025-01-31

### Authentication

#### JWT Bearer Token Authentication
All API requests require authentication using JWT (JSON Web Token) bearer tokens.

**Request Headers:**
```http
Authorization: Bearer <jwt_token>
Content-Type: application/json
Accept: application/json
User-Agent: <your_app_name>/<version>
```

**Token Structure:**
```json
{
  "sub": "user_12345",
  "iss": "https://api.crucible.app",
  "aud": "crucible_client",
  "exp": 1640995200,
  "iat": 1640991600,
  "scope": ["read:documents", "write:documents", "read:contacts"],
  "user_id": "user_12345",
  "role": "editor",
  "permissions": ["documents.create", "documents.update", "contacts.read"]
}
```

**Authentication Endpoints:**

##### POST /auth/login
Authenticate user credentials and receive JWT token.

**Request:**
```json
{
  "username": "sarah.chen",
  "password": "secure_password123",
  "remember_me": true,
  "client_info": {
    "app_name": "Crucible Desktop",
    "version": "2.1.0",
    "device_id": "device_abc123"
  }
}
```

**Response (200 OK):**
```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "token_type": "Bearer",
  "expires_in": 3600,
  "user": {
    "id": "user_12345",
    "username": "sarah.chen",
    "email": "sarah.chen@crucible.app",
    "role": "editor",
    "permissions": [
      "documents.create",
      "documents.update",
      "documents.delete",
      "contacts.read",
      "contacts.update"
    ]
  }
}
```

##### POST /auth/refresh
Refresh access token using refresh token.

**Request:**
```json
{
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
}
```

## Document Management Endpoints

### Documents Collection

#### GET /documents
Retrieve paginated list of documents with optional filtering and sorting.

**Query Parameters:**
- `page` (integer, optional): Page number (default: 1)
- `limit` (integer, optional): Items per page (default: 20, max: 100)
- `sort` (string, optional): Sort field (created, modified, title)
- `order` (string, optional): Sort order (asc, desc)
- `tags` (string, optional): Comma-separated tag list
- `search` (string, optional): Full-text search query
- `created_after` (date, optional): Filter by creation date
- `created_before` (date, optional): Filter by creation date
- `author` (string, optional): Filter by author ID

**Request Example:**
```http
GET /documents?page=1&limit=10&sort=created&order=desc&tags=knowledge-management&search=system%20architecture
```

**Response (200 OK):**
```json
{
  "data": [
    {
      "id": "doc_abc123",
      "title": "Knowledge Management System Architecture",
      "content": "# Knowledge Management System Architecture\n\nThis document outlines...",
      "frontmatter": {
        "type": "technical",
        "tags": ["knowledge-management", "architecture"],
        "created": "2025-01-15",
        "author": "Sarah Chen"
      },
      "tags": ["knowledge-management", "architecture"],
      "author": {
        "id": "user_12345",
        "name": "Sarah Chen",
        "email": "sarah.chen@crucible.app"
      },
      "created": "2025-01-15T10:30:00Z",
      "modified": "2025-01-20T14:22:00Z",
      "word_count": 1250,
      "reading_time": 5,
      "internal_links": 8,
      "external_links": 3
    }
  ],
  "pagination": {
    "page": 1,
    "limit": 10,
    "total": 47,
    "pages": 5,
    "has_next": true,
    "has_prev": false
  },
  "filters_applied": {
    "tags": ["knowledge-management"],
    "search": "system architecture"
  }
}
```

#### POST /documents
Create a new document.

**Request:**
```json
{
  "title": "New Technical Specification",
  "content": "# Technical Specification\n\nThis document contains...",
  "frontmatter": {
    "type": "technical",
    "tags": ["specification", "api"],
    "priority": "high",
    "status": "draft"
  },
  "tags": ["specification", "api"],
  "author_id": "user_12345"
}
```

**Response (201 Created):**
```json
{
  "id": "doc_def456",
  "title": "New Technical Specification",
  "content": "# Technical Specification\n\nThis document contains...",
  "frontmatter": {
    "type": "technical",
    "tags": ["specification", "api"],
    "priority": "high",
    "status": "draft",
    "created": "2025-01-31T15:45:00Z",
    "modified": "2025-01-31T15:45:00Z"
  },
  "tags": ["specification", "api"],
  "author": {
    "id": "user_12345",
    "name": "Sarah Chen"
  },
  "created": "2025-01-31T15:45:00Z",
  "modified": "2025-01-31T15:45:00Z",
  "word_count": 156,
  "reading_time": 1
}
```

### Individual Document Operations

#### GET /documents/{id}
Retrieve a specific document by ID.

**Response (200 OK):**
```json
{
  "id": "doc_abc123",
  "title": "Knowledge Management System Architecture",
  "content": "# Knowledge Management System Architecture\n\nThis document outlines...",
  "frontmatter": {
    "type": "technical",
    "tags": ["knowledge-management", "architecture"],
    "created": "2025-01-15",
    "author": "Sarah Chen",
    "version": "2.1"
  },
  "tags": ["knowledge-management", "architecture"],
  "author": {
    "id": "user_12345",
    "name": "Sarah Chen",
    "email": "sarah.chen@crucible.app"
  },
  "created": "2025-01-15T10:30:00Z",
  "modified": "2025-01-20T14:22:00Z",
  "version": 2,
  "word_count": 1250,
  "reading_time": 5,
  "internal_links": [
    {
      "target": "doc_def456",
      "text": "API Documentation",
      "type": "document"
    }
  ],
  "external_links": [
    {
      "url": "https://example.com/resource",
      "text": "External Resource"
    }
  ],
  "embeds": [
    {
      "target": "doc_ghi789",
      "type": "image",
      "alt_text": "Architecture Diagram"
    }
  ]
}
```

#### PUT /documents/{id}
Update an existing document.

**Request:**
```json
{
  "title": "Updated Technical Specification",
  "content": "# Updated Technical Specification\n\nThis document has been updated...",
  "frontmatter": {
    "type": "technical",
    "tags": ["specification", "api", "updated"],
    "priority": "high",
    "status": "published"
  },
  "tags": ["specification", "api", "updated"]
}
```

**Response (200 OK):**
```json
{
  "id": "doc_def456",
  "title": "Updated Technical Specification",
  "content": "# Updated Technical Specification\n\nThis document has been updated...",
  "frontmatter": {
    "type": "technical",
    "tags": ["specification", "api", "updated"],
    "priority": "high",
    "status": "published",
    "created": "2025-01-31T15:45:00Z",
    "modified": "2025-01-31T16:30:00Z",
    "version": "2"
  },
  "tags": ["specification", "api", "updated"],
  "modified": "2025-01-31T16:30:00Z",
  "version": 2
}
```

#### DELETE /documents/{id}
Delete a document.

**Response (204 No Content)**

## Search Endpoints

### Search Documents

#### GET /search/documents
Full-text search across document content, titles, and metadata.

**Query Parameters:**
- `q` (string, required): Search query
- `fields` (string, optional): Search fields (title,content,tags,all)
- `fuzzy` (boolean, optional): Enable fuzzy search (default: false)
- `highlight` (boolean, optional): Highlight matching terms (default: true)
- `limit` (integer, optional): Maximum results (default: 20)
- `offset` (integer, optional): Results offset (default: 0)

**Request Example:**
```http
GET /search/documents?q=knowledge%20management%20system&fields=all&fuzzy=true&highlight=true&limit=10
```

**Response (200 OK):**
```json
{
  "query": "knowledge management system",
  "results": [
    {
      "document": {
        "id": "doc_abc123",
        "title": "Knowledge Management System Architecture",
        "created": "2025-01-15T10:30:00Z",
        "author": "Sarah Chen"
      },
      "highlights": {
        "title": "<mark>Knowledge Management</mark> System Architecture",
        "content": "This document outlines the <mark>knowledge management</mark> <mark>system</mark>..."
      },
      "score": 0.95,
      "match_type": "full_text"
    }
  ],
  "total": 15,
  "search_time": 0.042,
  "suggestions": [
    "knowledge management framework",
    "information management system"
  ]
}
```

### Vector Search

#### POST /search/vector
Semantic search using vector embeddings for concept-based matching.

**Request:**
```json
{
  "query": "How to organize technical documentation effectively?",
  "limit": 10,
  "threshold": 0.7,
  "filters": {
    "tags": ["technical", "documentation"],
    "created_after": "2025-01-01"
  }
}
```

**Response (200 OK):**
```json
{
  "query_vector": [0.1234, -0.5678, 0.9012, ...],
  "results": [
    {
      "document": {
        "id": "doc_def456",
        "title": "Technical Documentation Best Practices",
        "similarity": 0.89
      },
      "explanation": "Semantic similarity based on document organization concepts"
    }
  ],
  "search_metadata": {
    "model": "text-embedding-3-large",
    "dimension": 3072,
    "search_time": 0.156
  }
}
```

## Contact Management Endpoints

### Contacts Collection

#### GET /contacts
Retrieve list of contacts with filtering options.

**Query Parameters:**
- `type` (string, optional): Contact type (internal, external, vendor)
- `department` (string, optional): Department filter
- `skills` (string, optional): Skill-based filter
- `location` (string, optional): Location filter

**Response (200 OK):**
```json
{
  "data": [
    {
      "id": "contact_123",
      "first_name": "Sarah",
      "last_name": "Chen",
      "email": "sarah.chen@crucible.app",
      "phone": "+1-555-123-4567",
      "type": "internal",
      "role": "Project Manager",
      "department": "Project Management",
      "location": "San Francisco, CA",
      "skills": ["agile", "stakeholder-management", "planning"],
      "projects": ["proj_001", "proj_002"],
      "created": "2023-03-15T00:00:00Z",
      "last_contact": "2025-01-30T14:20:00Z"
    }
  ],
  "total": 24,
  "filters_applied": {
    "type": "internal",
    "department": "Project Management"
  }
}
```

## Real-time Collaboration Endpoints

### WebSocket Connection

#### WebSocket /ws/collaborate/{document_id}
Real-time collaboration for document editing.

**Connection Parameters:**
- `token` (string, required): JWT authentication token
- `user_id` (string, required): User identifier

**Message Protocol:**

**Join Document:**
```json
{
  "type": "join",
  "document_id": "doc_abc123",
  "user_id": "user_12345"
}
```

**Operation Message:**
```json
{
  "type": "operation",
  "operation": {
    "type": "insert",
    "position": 156,
    "content": "New text content",
    "author": "user_12345",
    "timestamp": "2025-01-31T16:45:30Z"
  }
}
```

**Cursor Position:**
```json
{
  "type": "cursor",
  "user_id": "user_12345",
  "position": 156,
  "selection": {
    "start": 156,
    "end": 162
  }
}
```

## Error Handling

### Error Response Format

All error responses follow a consistent format:

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid request parameters",
    "details": {
      "field": "email",
      "issue": "Invalid email format"
    },
    "request_id": "req_abc123def456",
    "timestamp": "2025-01-31T16:45:30Z"
  }
}
```

### Common Error Codes

| Status Code | Error Code | Description |
|-------------|------------|-------------|
| 400 | VALIDATION_ERROR | Invalid request parameters |
| 401 | UNAUTHORIZED | Missing or invalid authentication |
| 403 | FORBIDDEN | Insufficient permissions |
| 404 | NOT_FOUND | Resource not found |
| 409 | CONFLICT | Resource conflict (duplicate, etc.) |
| 422 | UNPROCESSABLE_ENTITY | Semantic validation errors |
| 429 | RATE_LIMIT_EXCEEDED | Too many requests |
| 500 | INTERNAL_SERVER_ERROR | Server error |

### Rate Limiting

**Rate Limit Headers:**
```http
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 947
X-RateLimit-Reset: 1640995200
```

**Rate Limit Exceeded Response (429):**
```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Rate limit exceeded. Try again later.",
    "retry_after": 60,
    "limit": 1000,
    "window": 3600
  }
}
```

## Integration Examples

### JavaScript/TypeScript Client

```typescript
interface CrucibleAPIClient {
  authenticate(credentials: LoginCredentials): Promise<AuthResponse>;
  getDocuments(filters?: DocumentFilters): Promise<DocumentList>;
  createDocument(document: CreateDocumentRequest): Promise<Document>;
  updateDocument(id: string, updates: UpdateDocumentRequest): Promise<Document>;
  searchDocuments(query: string, options?: SearchOptions): Promise<SearchResults>;
}

class CrucibleClient implements CrucibleAPIClient {
  private baseURL = 'https://api.crucible.app/v2';
  private token: string | null = null;

  async authenticate(credentials: LoginCredentials): Promise<AuthResponse> {
    const response = await fetch(`${this.baseURL}/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(credentials)
    });

    if (!response.ok) {
      throw new Error(`Authentication failed: ${response.statusText}`);
    }

    const auth = await response.json();
    this.token = auth.access_token;
    return auth;
  }

  async getDocuments(filters?: DocumentFilters): Promise<DocumentList> {
    return this.makeRequest('/documents', 'GET', null, filters);
  }

  private async makeRequest<T>(
    endpoint: string,
    method: string = 'GET',
    body?: any,
    params?: any
  ): Promise<T> {
    if (!this.token) {
      throw new Error('Not authenticated');
    }

    const url = new URL(`${this.baseURL}${endpoint}`);
    if (params) {
      Object.entries(params).forEach(([key, value]) => {
        if (value !== undefined) {
          url.searchParams.append(key, String(value));
        }
      });
    }

    const response = await fetch(url.toString(), {
      method,
      headers: {
        'Authorization': `Bearer ${this.token}`,
        'Content-Type': 'application/json'
      },
      body: body ? JSON.stringify(body) : undefined
    });

    if (!response.ok) {
      const error = await response.json();
      throw new Error(error.error?.message || `API Error: ${response.statusText}`);
    }

    return response.json();
  }
}
```

### Python Client

```python
import requests
from typing import Optional, Dict, Any, List
from dataclasses import dataclass

@dataclass
class CrucibleClient:
    base_url: str = "https://api.crucible.app/v2"
    token: Optional[str] = None

    def authenticate(self, username: str, password: str) -> Dict[str, Any]:
        """Authenticate and store JWT token"""
        response = requests.post(
            f"{self.base_url}/auth/login",
            json={"username": username, "password": password}
        )
        response.raise_for_status()

        auth_data = response.json()
        self.token = auth_data["access_token"]
        return auth_data

    def get_documents(self, **filters) -> Dict[str, Any]:
        """Get documents with optional filters"""
        return self._make_request("GET", "/documents", params=filters)

    def create_document(self, document: Dict[str, Any]) -> Dict[str, Any]:
        """Create a new document"""
        return self._make_request("POST", "/documents", json=document)

    def search_documents(self, query: str, **options) -> Dict[str, Any]:
        """Search documents"""
        params = {"q": query, **options}
        return self._make_request("GET", "/search/documents", params=params)

    def _make_request(
        self,
        method: str,
        endpoint: str,
        json: Optional[Dict] = None,
        params: Optional[Dict] = None
    ) -> Dict[str, Any]:
        """Make authenticated API request"""
        if not self.token:
            raise ValueError("Not authenticated - call authenticate() first")

        headers = {
            "Authorization": f"Bearer {self.token}",
            "Content-Type": "application/json"
        }

        response = requests.request(
            method,
            f"{self.base_url}{endpoint}",
            headers=headers,
            json=json,
            params=params
        )
        response.raise_for_status()
        return response.json()
```

## Integration with System Documentation

This API documentation connects to:

- [[Technical Documentation]] for implementation details and code examples
- [[Project Management]] for API development tracking
- [[Contact Management]] for API user management
- [[Knowledge Management Hub]] for API usage patterns and best practices

## Search Testing Targets

This API documentation enables testing of technical search capabilities:

- **API Operations**: "GET", "POST", "PUT", "DELETE", "authentication"
- **Endpoint Types**: "documents", "search", "contacts", "collaboration"
- **HTTP Status Codes**: "200 OK", "201 Created", "404 Not Found", "500 Error"
- **Authentication**: "JWT", "Bearer token", "rate limiting", "permissions"
- **Data Formats**: "JSON", "request", "response", "headers"
- **Integration**: "JavaScript", "Python", "TypeScript", "REST API"

## API Roadmap

### Upcoming Features (Q2 2025)
- **GraphQL Endpoint**: Alternative query interface for complex data fetching
- **Bulk Operations**: Batch create/update/delete operations
- **Webhook Support**: Event-driven notifications for document changes
- **Advanced Search**: Natural language queries and faceted search

### Future Enhancements (Q3-Q4 2025)
- **API Analytics**: Usage metrics and performance insights
- **Custom Endpoints**: User-defined API extensions
- **Real-time Streaming**: Server-sent events for live updates
- **Machine Learning APIs**: Content analysis and recommendation endpoints

---

*This API documentation note provides comprehensive technical specifications, integration examples, and implementation guidelines for testing API documentation, code examples, and technical content search features.*