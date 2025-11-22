# Software Development Workflow

This workflow demonstrates a collaborative software development process with multiple agents working across different channels, showing hierarchical task breakdown and complex data flows.

## Phase 1: Planning

The product team defines requirements and creates specifications.

### Gather Requirements @product-manager #planning
Interviews stakeholders and documents feature requests.
stakeholder_input::Interviews → requirements_doc::Markdown

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,stakeholders}:
 GatherRequirements,product-manager,planning,interviews.md,requirements.md,12000,success,4200,5
```

### Create Specification @product-manager #planning
Transforms requirements into technical specification.
requirements_doc → product_spec::Markdown

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used}:
 CreateSpecification,product-manager,planning,requirements.md,product_spec.md,8500,success,3800
```

---

## Phase 2: Design

Architecture and design team creates technical designs.

### Architecture Design @architect #design
Creates high-level system architecture.
product_spec → architecture_diagram::SVG, tech_decisions::Markdown

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,diagrams_created}:
 ArchitectureDesign,architect,design,product_spec.md,arch.svg,15000,success,5200,3
```

### Database Schema @data-architect #design
Designs database schema and migrations.
tech_decisions → schema_design::SQL, migrations::SQL[]

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,tables_created}:
 DatabaseSchema,data-architect,design,tech_decisions.md,schema.sql,9500,success,3400,12
```

### API Design @api-architect #design
Defines API contracts and endpoints.
architecture_diagram → api_spec::OpenAPI

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,endpoints}:
 APIDesign,api-architect,design,arch.svg,api_spec.yaml,11000,success,4100,23
```

---

## Phase 3: Implementation

Development team implements the designed features in parallel.

### Setup Infrastructure @devops-engineer #infrastructure !
Critical: Configures cloud resources and CI/CD.
architecture_diagram, schema_design → infrastructure_code::Terraform

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,resources}:
 SetupInfrastructure,devops-engineer,infrastructure,arch.svg,infra.tf,18000,success,6200,15
```

### Backend Development @backend-developer #development
Implements API endpoints and business logic.
api_spec, schema_design → backend_code::TypeScript[]

Multiple developers work on different features concurrently.

```session-toon
execution[4]{phase,agent,channel,input,output,duration_ms,status,tokens_used,files_changed,feature}:
 BackendDevelopment,backend-dev-1,development,api_spec.yaml,auth.ts,45000,success,12000,8,authentication
 BackendDevelopment,backend-dev-2,development,api_spec.yaml,orders.ts,52000,success,15000,12,order-management
 BackendDevelopment,backend-dev-3,development,api_spec.yaml,payments.ts,38000,success,11000,6,payment-processing
 BackendDevelopment,backend-dev-4,development,api_spec.yaml,notifications.ts,28000,success,8500,5,notifications
```

### Frontend Development @frontend-developer #development
Builds user interface components.
api_spec → frontend_code::React[]

```session-toon
execution[3]{phase,agent,channel,input,output,duration_ms,status,tokens_used,components,feature}:
 FrontendDevelopment,frontend-dev-1,development,api_spec.yaml,cart.tsx,42000,success,13000,7,shopping-cart
 FrontendDevelopment,frontend-dev-2,development,api_spec.yaml,checkout.tsx,48000,success,14500,9,checkout-flow
 FrontendDevelopment,frontend-dev-3,development,api_spec.yaml,account.tsx,35000,success,10500,6,user-account
```

---

## Phase 4: Testing

QA team validates implementation against specifications.

### Unit Tests @test-engineer #testing !
Critical: Writes and runs automated unit tests.
backend_code, frontend_code → test_results::JUnit, coverage_report::HTML

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,tests_run,failures,coverage_pct}:
 UnitTests,test-engineer,testing,src/,test_results.xml,120000,success,8500,342,0,87.5
```

### Integration Tests @test-engineer #testing !
Critical: Tests API endpoints and database interactions.
backend_code, infrastructure_code → integration_results::JUnit

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,tests_run,failures}:
 IntegrationTests,test-engineer,testing,api_spec.yaml,integration.xml,180000,success,12000,45,2
```

### Fix Test Failures @backend-developer #development
Addresses failing integration tests.
integration_results → fixed_code::TypeScript[]

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,fixes}:
 FixTestFailures,backend-dev-2,development,integration.xml,orders.ts,15000,success,4200,2
```

### Re-run Integration Tests @test-engineer #testing !
Critical: Validates fixes resolved issues.
fixed_code → final_test_results::JUnit

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,tests_run,failures}:
 RerunIntegrationTests,test-engineer,testing,api_spec.yaml,integration_final.xml,180000,success,12000,45,0
```

---

## Phase 5: Code Review

Senior engineers review code quality and security.

### Backend Code Review @senior-backend #code-review
Reviews backend implementation for quality and security.
backend_code → review_feedback::Markdown, approval_status::Boolean

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,comments,approved}:
 BackendCodeReview,senior-backend,code-review,src/backend/,review_backend.md,25000,success,8900,12,true
```

### Frontend Code Review @senior-frontend #code-review
Reviews frontend components for best practices.
frontend_code → review_feedback::Markdown, approval_status::Boolean

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,comments,approved}:
 FrontendCodeReview,senior-frontend,code-review,src/frontend/,review_frontend.md,22000,success,7800,8,true
```

### Security Review @security-engineer #security !
Critical: Checks for security vulnerabilities.
backend_code, frontend_code → security_report::Markdown, vulnerabilities::Integer

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,critical,high,medium,low}:
 SecurityReview,security-engineer,security,src/,security_report.md,45000,success,15000,0,1,3,5
```

### Fix Security Issues @backend-developer #development !
Critical: Addresses security vulnerabilities.
security_report → secured_code::TypeScript[]

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,vulnerabilities_fixed}:
 FixSecurityIssues,backend-dev-1,development,security_report.md,auth.ts,18000,success,5200,4
```

---

## Phase 6: Deployment

Operations team deploys to production environment.

### Build Release @ci-system #build !
Critical: Compiles and packages application.
secured_code, infrastructure_code → build_artifacts::Docker[]

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,images}:
 BuildRelease,ci-system,build,src/,docker_images,240000,success,3200,4
```

### Deploy Staging @devops-engineer #infrastructure ?
Optional: Deploys to staging environment for final validation.
build_artifacts → staging_url::URL

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,url}:
 DeployStaging,devops-engineer,infrastructure,docker_images,https://staging.example.com,95000,success,4800,1
```

### Smoke Tests @test-engineer #testing
Runs basic health checks on staging deployment.
staging_url → smoke_test_results::Boolean

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,tests_run,failures}:
 SmokeTests,test-engineer,testing,https://staging.example.com,smoke_results.xml,30000,success,2400,15,0
```

### Deploy Production @devops-engineer #production !
Critical: Deploys to production environment.
build_artifacts, smoke_test_results → production_url::URL

Requires manual approval before proceeding.

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,url,approval_by}:
 DeployProduction,devops-engineer,production,docker_images,https://app.example.com,120000,success,5200,1,john.doe@example.com
```

### Monitor Deployment @sre-engineer #monitoring
Monitors production metrics for anomalies.
production_url → monitoring_report::Markdown

```session-toon
execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used,alerts}:
 MonitorDeployment,sre-engineer,monitoring,https://app.example.com,metrics.md,1800000,success,6500,0
```

---

## Workflow Retrospective

### Summary Statistics

**Team Collaboration**:
- Total agents involved: 14
- Channels used: 7 (planning, design, development, testing, code-review, security, production)
- Total execution time: ~28 hours (wallclock: 4 days with parallel work)

**Development Metrics**:
- Requirements → Production: 4 days
- Code files changed: 46
- Tests written: 402
- Test coverage: 87.5%
- Security vulnerabilities fixed: 4 (1 high, 3 medium)

**Parallel Processing Benefits**:
- Backend and frontend developed concurrently (saved ~2 days)
- Multiple backend developers on different features (saved ~3 days)
- Multiple code reviews run in parallel (saved ~4 hours)

**Critical Path**:
The longest sequential path was:
1. Gather Requirements (12s)
2. Create Specification (8.5s)
3. Architecture Design (15s)
4. Setup Infrastructure (18s)
5. Backend Development (longest: 52s)
6. Integration Tests (180s)
7. Security Review (45s)
8. Build Release (240s)
9. Deploy Production (120s)

Total critical path: ~691 seconds (~11.5 minutes of sequential work)

**Bottlenecks Identified**:
- Build release took longest (240s) - consider caching dependencies
- Integration tests took 180s - could parallelize test suites
- Security review manual process - investigate automated scanning tools

**Next Steps**:
- Implement automated security scanning in CI pipeline
- Optimize build process with layer caching
- Parallelize integration test execution
- Add automated rollback on production anomalies
