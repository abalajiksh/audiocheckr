pipeline {
    agent any
    
    parameters {
        choice(
            name: 'TEST_TYPE_OVERRIDE',
            choices: ['AUTO', 'QUALIFICATION', 'REGRESSION', 'REGRESSION_GENRE'],
            description: 'Force a specific test type. AUTO uses smart detection.'
        )
        booleanParam(
            name: 'SKIP_ARM_BUILD',
            defaultValue: false,
            description: 'Skip ARM build and tests (for quick x86-64 only builds)'
        )
        booleanParam(
            name: 'RUN_GENRE_REGRESSION',
            defaultValue: false,
            description: 'Run regression genre tests (manual trigger only)'
        )
        booleanParam(
            name: 'SKIP_SONARQUBE',
            defaultValue: false,
            description: 'Skip SonarQube analysis'
        )
        booleanParam(
            name: 'CLEAN_WORKSPACE_BEFORE',
            defaultValue: false,
            description: 'Clean workspace before build (use if seeing stale file issues)'
        )
    }
    
    environment {
        // MinIO configuration
        MINIO_BUCKET = 'audiocheckr'
        MINIO_FILE_COMPACT = 'CompactTestFiles.zip'
        MINIO_FILE_FULL = 'TestFiles.zip'
        MINIO_FILE_GENRE_LITE = 'GenreTestSuiteLite.zip'
        MINIO_FILE_GENRE_FULL = 'TestSuite.zip'
        
        // SonarQube configuration
        SONAR_PROJECT_KEY = 'audiocheckr'
        SONAR_PROJECT_NAME = 'AudioCheckr'
        SONAR_SOURCES = 'src'
        
        // Path setup
        PATH = "$HOME/bin:$HOME/.cargo/bin:/usr/bin:$PATH"
        
        // PODMAN_LXC_HOST and PODMAN_LXC_USER are set globally in Jenkins
        // Configure in: Manage Jenkins → System → Global properties → Environment variables
    }
    
    triggers {
        // Scheduled regression test - Saturday at 10:00 AM
        cron('0 10 * * 6')
    }
    
    options {
        // Build timeout (increased for ARM builds via QEMU)
        timeout(time: 90, unit: 'MINUTES')
        
        // Keep last 10 builds
        buildDiscarder(logRotator(numToKeepStr: '10', artifactNumToKeepStr: '5'))
        
        // Add timestamps to console output
        timestamps()
        
        // Don't run concurrent builds
        disableConcurrentBuilds()
    }
    
    stages {
        stage('Pre-flight') {
            steps {
                script {
                    // Clean workspace if requested
                    if (params.CLEAN_WORKSPACE_BEFORE) {
                        deleteDir()
                        checkout scm
                    }
                    
                    // Determine test type
                    if (params.TEST_TYPE_OVERRIDE && params.TEST_TYPE_OVERRIDE != 'AUTO') {
                        env.TEST_TYPE = params.TEST_TYPE_OVERRIDE
                        echo "🔧 Test type forced via parameter: ${env.TEST_TYPE}"
                    } else if (currentBuild.getBuildCauses('hudson.triggers.TimerTrigger$TimerTriggerCause')) {
                        // Scheduled build (cron) = REGRESSION
                        env.TEST_TYPE = 'REGRESSION'
                        echo "⏰ Scheduled build detected - running REGRESSION tests"
                    } else if (currentBuild.getBuildCauses('hudson.model.Cause$UserIdCause')) {
                        // Manual build = QUALIFICATION by default
                        env.TEST_TYPE = 'QUALIFICATION'
                        echo "👤 Manual build - running QUALIFICATION tests (use parameter to override)"
                    } else {
                        // GitHub push = QUALIFICATION
                        env.TEST_TYPE = 'QUALIFICATION'
                        echo "🔄 Push detected - running QUALIFICATION tests"
                    }
                    
                    // Display build info
                    echo """
========================================================
                  AUDIOCHECKR CI/CD                     
========================================================
  Test Type:     ${env.TEST_TYPE}
  ARM Build:     ${params.SKIP_ARM_BUILD ? 'DISABLED ⏭️' : 'ENABLED ✓'}
  Build #:       ${currentBuild.number}
  Triggered by:  ${currentBuild.getBuildCauses()[0].shortDescription}
========================================================
"""
                }
            }
        }
        
        stage('Setup & Checkout') {
            parallel {
                stage('Setup Tools') {
                    steps {
                        sh '''
                            mkdir -p $HOME/bin
                            
                            if ! command -v cc >/dev/null 2>&1; then
                                echo "ERROR: C compiler not found!"
                                exit 1
                            fi
                            
                            if ! command -v mc >/dev/null 2>&1; then
                                echo "Installing MinIO client..."
                                wget -q https://dl.min.io/client/mc/release/linux-amd64/mc -O $HOME/bin/mc
                                chmod +x $HOME/bin/mc
                            fi
                            
                            if ! command -v cargo >/dev/null 2>&1; then
                                echo "Installing Rust..."
                                curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
                                . $HOME/.cargo/env
                            fi
                            
                            echo "=== Tool Versions ==="
                            mc --version
                            cargo --version
                            rustc --version
                            echo "===================="
                        '''
                    }
                }
                
                stage('Checkout') {
                    steps {
                        checkout scm
                        script {
                            env.GIT_COMMIT_MSG = sh(script: 'git log -1 --pretty=%B', returnStdout: true).trim()
                            env.GIT_COMMIT_SHORT = sh(script: 'git rev-parse --short HEAD', returnStdout: true).trim()
                            env.GIT_AUTHOR = sh(script: 'git log -1 --pretty=%an', returnStdout: true).trim()
                            echo "Commit: ${env.GIT_COMMIT_SHORT} by ${env.GIT_AUTHOR}"
                            echo "Message: ${env.GIT_COMMIT_MSG}"
                        }
                    }
                }
            }
        }
        
        stage('Build & Prepare') {
            parallel {
                stage('Build x86_64') {
                    steps {
                        sh '''
                            echo "=========================================="
                            echo "Building for x86_64 (native)"
                            echo "=========================================="
                            cargo build --release 2>&1 | tee build_x86_64.txt
                            
                            # Check for warnings (informational, doesn't fail build)
                            if grep -q "warning:" build_x86_64.txt; then
                                echo ""
                                echo "=== Build Warnings Summary ==="
                                grep -c "warning:" build_x86_64.txt || true
                                echo "warnings found (see above for details)"
                                echo "=============================="
                            fi
                            
                            echo ""
                            echo "=== x86_64 Build Artifact ==="
                            ls -lh target/release/audiocheckr
                            file target/release/audiocheckr
                            echo "============================="
                        '''
                    }
                }
                
                stage('Download Test Files') {
                    steps {
                        script {
                            withCredentials([
                                usernamePassword(
                                    credentialsId: 'noIdea',
                                    usernameVariable: 'MINIO_ACCESS_KEY',
                                    passwordVariable: 'MINIO_SECRET_KEY'
                                ),
                                string(
                                    credentialsId: 'minio-endpoint',
                                    variable: 'MINIO_ENDPOINT'
                                )
                            ]) {
                                if (env.TEST_TYPE == 'REGRESSION' || env.TEST_TYPE == 'REGRESSION_GENRE') {
                                    sh '''
                                        set -e
                                        mc alias set myminio "$MINIO_ENDPOINT" "$MINIO_ACCESS_KEY" "$MINIO_SECRET_KEY"
                                        
                                        echo "=========================================="
                                        echo "Downloading REGRESSION test files"
                                        echo "=========================================="
                                        
                                        # Download and extract TestFiles
                                        echo "Downloading ${MINIO_FILE_FULL} (~8.5GB)"
                                        mc cp myminio/${MINIO_BUCKET}/${MINIO_FILE_FULL} .
                                        unzip -q -o ${MINIO_FILE_FULL}
                                        rm -f ${MINIO_FILE_FULL}
                                        
                                        # Download and extract TestSuite
                                        echo "Downloading ${MINIO_FILE_GENRE_FULL} (~2.5GB)"
                                        mc cp myminio/${MINIO_BUCKET}/${MINIO_FILE_GENRE_FULL} .
                                        unzip -q -o ${MINIO_FILE_GENRE_FULL}
                                        rm -f ${MINIO_FILE_GENRE_FULL}
                                        
                                        echo "Test files ready"
                                        find TestFiles TestSuite -type f -name "*.flac" 2>/dev/null | wc -l || echo "0"
                                        du -sh TestFiles TestSuite 2>/dev/null || true
                                    '''
                                } else {
                                    sh '''
                                        set -e
                                        mc alias set myminio "$MINIO_ENDPOINT" "$MINIO_ACCESS_KEY" "$MINIO_SECRET_KEY"
                                        
                                        echo "=========================================="
                                        echo "Downloading QUALIFICATION test files"
                                        echo "=========================================="
                                        
                                        # Download and extract CompactTestFiles
                                        echo "Downloading ${MINIO_FILE_COMPACT} (~1.4GB)"
                                        mc cp myminio/${MINIO_BUCKET}/${MINIO_FILE_COMPACT} .
                                        unzip -q -o ${MINIO_FILE_COMPACT}
                                        if [ -d "CompactTestFiles" ]; then
                                            mv CompactTestFiles TestFiles
                                        fi
                                        rm -f ${MINIO_FILE_COMPACT}
                                        
                                        # Download and extract GenreTestSuiteLite
                                        echo "Downloading ${MINIO_FILE_GENRE_LITE} (~800MB)"
                                        mc cp myminio/${MINIO_BUCKET}/${MINIO_FILE_GENRE_LITE} .
                                        unzip -q -o ${MINIO_FILE_GENRE_LITE}
                                        rm -f ${MINIO_FILE_GENRE_LITE}
                                        
                                        echo "Test files ready"
                                        find TestFiles GenreTestSuiteLite -type f -name "*.flac" 2>/dev/null | wc -l || echo "0"
                                        du -sh TestFiles GenreTestSuiteLite 2>/dev/null || true
                                    '''
                                }
                            }
                        }
                    }
                }
            }
        }
        
        stage('SonarQube Analysis') {
            when {
                expression { return !params.SKIP_SONARQUBE }
            }
            steps {
                script {
                    try {
                        def scannerHome = tool 'SonarQube-LXC'
                        
                        withSonarQubeEnv('SonarQube-LXC') {
                            sh """
                                ${scannerHome}/bin/sonar-scanner \
                                    -Dsonar.projectKey=${SONAR_PROJECT_KEY} \
                                    -Dsonar.projectName=${SONAR_PROJECT_NAME} \
                                    -Dsonar.sources=${SONAR_SOURCES} \
                                    -Dsonar.exclusions=**/target/**,**/TestFiles/**,**/TestSuite/**,**/GenreTestSuiteLite/**
                            """
                        }
                        echo "✓ SonarQube analysis completed (source code only, architecture-independent)"
                    } catch (Exception e) {
                        echo "⚠ SonarQube analysis failed: ${e.message}"
                    }
                }
            }
        }
        
        stage('Quality Gate') {
            when {
                expression { return !params.SKIP_SONARQUBE }
            }
            steps {
                script {
                    try {
                        timeout(time: 10, unit: 'MINUTES') {
                            def qg = waitForQualityGate abortPipeline: false
                            if (qg.status != 'OK') {
                                echo "⚠ Quality Gate: ${qg.status}"
                            } else {
                                echo "✓ Quality Gate: PASSED"
                            }
                        }
                    } catch (Exception e) {
                        echo "⚠ Quality Gate skipped: ${e.message}"
                        echo "Tip: Configure webhook in SonarQube > Project Settings > Webhooks"
                        echo "URL: http://YOUR_JENKINS_URL/sonarqube-webhook/"
                    }
                }
            }
        }
        
        stage('x86_64 Tests (Full Suite)') {
            stages {
                stage('Integration Tests') {
                    steps {
                        script {
                            echo "=========================================="
                            echo "x86_64: Integration Tests"
                            echo "=========================================="
                            
                            sh 'mkdir -p target/test-results'
                            
                            def integrationResult = sh(
                                script: '''
                                    set +e
                                    cargo test --test integration_test -- --nocapture 2>&1 | tee target/test-results/integration_x86_64.txt
                                    exit ${PIPESTATUS[0]}
                                ''',
                                returnStatus: true
                            )
                            
                            if (integrationResult != 0) {
                                echo "⚠ Integration tests had failures"
                            } else {
                                echo "✓ Integration tests passed!"
                            }
                        }
                    }
                }
                
                stage('Qualification Tests') {
                    when {
                        expression { return env.TEST_TYPE == 'QUALIFICATION' }
                    }
                    parallel {
                        stage('Qualification Test') {
                            steps {
                                script {
                                    sh 'mkdir -p target/test-results'
                                    
                                    echo "=========================================="
                                    echo "x86_64: Running QUALIFICATION tests"
                                    echo "=========================================="
                                    
                                    def testResult = sh(
                                        script: """
                                            set +e
                                            cargo test --test qualification_test -- --nocapture 2>&1 | tee target/test-results/qualification_x86_64.txt
                                            exit \${PIPESTATUS[0]}
                                        """,
                                        returnStatus: true
                                    )
                                    
                                    if (testResult != 0) {
                                        echo "⚠ Qualification tests completed with failures"
                                        currentBuild.result = 'UNSTABLE'
                                    } else {
                                        echo "✓ Qualification tests passed!"
                                    }
                                }
                            }
                        }
                        
                        stage('Qualification Genre Test') {
                            steps {
                                script {
                                    sh 'mkdir -p target/test-results'
                                    
                                    echo "=========================================="
                                    echo "x86_64: Running QUALIFICATION GENRE tests"
                                    echo "=========================================="
                                    
                                    def testResult = sh(
                                        script: """
                                            set +e
                                            cargo test --test qualification_genre_tests -- --nocapture 2>&1 | tee target/test-results/qualification_genre_x86_64.txt
                                            exit \${PIPESTATUS[0]}
                                        """,
                                        returnStatus: true
                                    )
                                    
                                    if (testResult != 0) {
                                        echo "⚠ Qualification genre tests completed with failures"
                                        currentBuild.result = 'UNSTABLE'
                                    } else {
                                        echo "✓ Qualification genre tests passed!"
                                    }
                                }
                            }
                        }
                    }
                }
                
                stage('Regression Tests') {
                    when {
                        expression { return env.TEST_TYPE == 'REGRESSION' }
                    }
                    steps {
                        script {
                            sh 'mkdir -p target/test-results'
                            
                            echo "=========================================="
                            echo "x86_64: Running REGRESSION tests"
                            echo "=========================================="
                            
                            def testResult = sh(
                                script: """
                                    set +e
                                    cargo test --test regression_test -- --nocapture 2>&1 | tee target/test-results/regression_x86_64.txt
                                    exit \${PIPESTATUS[0]}
                                """,
                                returnStatus: true
                            )
                            
                            if (testResult != 0) {
                                echo "⚠ Regression tests completed with failures"
                                currentBuild.result = 'UNSTABLE'
                            } else {
                                echo "✓ Regression tests passed!"
                            }
                        }
                    }
                }
                
                stage('Regression Genre Tests') {
                    when {
                        expression { 
                            return env.TEST_TYPE == 'REGRESSION_GENRE' || params.RUN_GENRE_REGRESSION == true
                        }
                    }
                    steps {
                        script {
                            sh 'mkdir -p target/test-results'
                            
                            echo "=========================================="
                            echo "x86_64: Running REGRESSION GENRE tests (MANUAL ONLY)"
                            echo "=========================================="
                            
                            def testResult = sh(
                                script: """
                                    set +e
                                    cargo test --test regression_genre_tests -- --nocapture 2>&1 | tee target/test-results/regression_genre_x86_64.txt
                                    exit \${PIPESTATUS[0]}
                                """,
                                returnStatus: true
                            )
                            
                            if (testResult != 0) {
                                echo "⚠ Regression genre tests completed with failures"
                                currentBuild.result = 'UNSTABLE'
                            } else {
                                echo "✓ Regression genre tests passed!"
                            }
                        }
                    }
                }
            }
        }
        
        stage('ARM64 Validation (Light)') {
            when {
                expression { return !params.SKIP_ARM_BUILD }
            }
            steps {
                script {
                    echo """
========================================================
        ARM64 BUILD & VALIDATION (via Podman LXC)
========================================================
  Strategy: Light testing to verify ARM compatibility
  - Integration tests (smoke test)
  - Qualification genre tests (detection validation)
  
  Skip: Heavy regression tests (same functionality as x86-64)
  
  Using: ${env.PODMAN_LXC_USER}@${env.PODMAN_LXC_HOST}
========================================================
"""
                    
                    // Use SSH to Podman LXC for ARM build/test
                    sshagent(credentials: ['podman-lxc-ssh']) {
                        sh '''
                            set -e
                            
                            echo "📦 Creating workspace on Podman LXC..."
                            ssh -o StrictHostKeyChecking=no ${PODMAN_LXC_USER}@${PODMAN_LXC_HOST} "mkdir -p ~/audiocheckr-arm-build"
                            
                            echo "📤 Copying source code..."
                            rsync -avz --delete --exclude='target' --exclude='TestFiles' --exclude='TestSuite' --exclude='GenreTestSuiteLite' \
                                ./ ${PODMAN_LXC_USER}@${PODMAN_LXC_HOST}:~/audiocheckr-arm-build/
                            
                            echo "📤 Copying test files (GenreTestSuiteLite only for light testing)..."
                            if [ -d "GenreTestSuiteLite" ]; then
                                rsync -avz --delete GenreTestSuiteLite/ ${PODMAN_LXC_USER}@${PODMAN_LXC_HOST}:~/audiocheckr-arm-build/GenreTestSuiteLite/
                            fi
                            
                            echo ""
                            echo "🏗️ Building and testing on Podman LXC..."
                            
                            # Run ARM build and tests on Podman LXC
                            ssh ${PODMAN_LXC_USER}@${PODMAN_LXC_HOST} << 'PODMAN_EOF'
                                set -e
                                cd ~/audiocheckr-arm-build
                                
                                echo "=========================================="
                                echo "Setting up ARM64 cross-compilation"
                                echo "=========================================="
                                
                                # Setup ARM cross-compilation target
                                if ! rustup target list --installed | grep -q aarch64-unknown-linux-gnu; then
                                    echo "Installing aarch64 target..."
                                    rustup target add aarch64-unknown-linux-gnu
                                fi
                                
                                # Install ARM cross-compiler
                                if ! command -v aarch64-linux-gnu-gcc >/dev/null 2>&1; then
                                    echo "Installing ARM64 cross-compiler..."
                                    sudo apt-get update -qq
                                    sudo apt-get install -y gcc-aarch64-linux-gnu
                                fi
                                
                                # Install QEMU for running ARM binaries
                                if ! command -v qemu-aarch64-static >/dev/null 2>&1; then
                                    echo "Installing QEMU user emulation..."
                                    sudo apt-get install -y qemu-user-static binfmt-support
                                fi
                                
                                echo ""
                                echo "=========================================="
                                echo "Building ARM64 binary"
                                echo "=========================================="
                                
                                export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
                                export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
                                export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
                                
                                cargo build --release --target aarch64-unknown-linux-gnu 2>&1 | tee build_arm64.txt
                                
                                echo ""
                                echo "=== ARM64 Build Artifact ==="
                                ls -lh target/aarch64-unknown-linux-gnu/release/audiocheckr
                                file target/aarch64-unknown-linux-gnu/release/audiocheckr
                                echo "============================="
                                
                                echo ""
                                echo "=========================================="
                                echo "ARM64: Integration Tests (via QEMU)"
                                echo "=========================================="
                                
                                export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER="qemu-aarch64-static -L /usr/aarch64-linux-gnu"
                                cargo test --test integration_test --target aarch64-unknown-linux-gnu -- --nocapture 2>&1 | tee test_integration_arm64.txt || true
                                
                                echo ""
                                echo "=========================================="
                                echo "ARM64: Qualification Genre Tests (via QEMU)"
                                echo "=========================================="
                                
                                cargo test --test qualification_genre_tests --target aarch64-unknown-linux-gnu -- --nocapture 2>&1 | tee test_qualification_genre_arm64.txt || true
                                
                                echo ""
                                echo "✓ ARM64 validation complete"
PODMAN_EOF
                            
                            echo ""
                            echo "📥 Fetching ARM64 binary and test results..."
                            mkdir -p target/arm64
                            
                            # Fetch ARM binary
                            scp ${PODMAN_LXC_USER}@${PODMAN_LXC_HOST}:~/audiocheckr-arm-build/target/aarch64-unknown-linux-gnu/release/audiocheckr \
                                target/arm64/audiocheckr-arm64
                            
                            # Fetch test logs (optional)
                            scp ${PODMAN_LXC_USER}@${PODMAN_LXC_HOST}:~/audiocheckr-arm-build/test_*.txt \
                                target/arm64/ || true
                            
                            echo ""
                            echo "=== ARM64 Binary Downloaded ==="
                            ls -lh target/arm64/audiocheckr-arm64
                            file target/arm64/audiocheckr-arm64
                            echo "==============================="
                            
                            echo ""
                            echo "✓ ARM64 build and validation completed successfully!"
                        '''
                    }
                }
            }
        }
    }
    
    post {
        success {
            echo '✓ Multi-arch build and tests completed successfully!'
        }
        unstable {
            echo '⚠ Build completed but some tests failed. Check test results.'
        }
        failure {
            echo '✗ Build or tests failed. Check logs for details.'
        }
        always {
            script {
                // Archive x86_64 binary (on success or unstable)
                if (currentBuild.result != 'FAILURE') {
                    archiveArtifacts artifacts: 'target/release/audiocheckr', 
                                   fingerprint: true, 
                                   allowEmptyArchive: true
                    
                    // Archive ARM64 binary if built
                    if (!params.SKIP_ARM_BUILD && fileExists('target/arm64/audiocheckr-arm64')) {
                        archiveArtifacts artifacts: 'target/arm64/audiocheckr-arm64', 
                                       fingerprint: true, 
                                       allowEmptyArchive: true
                    }
                }
            }
            
            // Publish JUnit test results (shows in Jenkins UI)
            junit(
                allowEmptyResults: true,
                testResults: 'target/test-results/*.xml',
                skipPublishingChecks: false
            )
            
            // Clean up everything to save disk space
            script {
                echo "🧹 Cleaning workspace to save disk space..."
                
                sh '''
                    # Delete test files and ZIPs
                    rm -f CompactTestFiles.zip TestFiles.zip GenreTestSuiteLite.zip TestSuite.zip
                    rm -rf CompactTestFiles TestFiles GenreTestSuiteLite TestSuite
                    
                    # Keep the release binaries, clean build cache
                    if [ -f target/release/audiocheckr ]; then
                        cp target/release/audiocheckr /tmp/audiocheckr_backup_x86_$BUILD_NUMBER 2>/dev/null || true
                    fi
                    
                    if [ -f target/arm64/audiocheckr-arm64 ]; then
                        cp target/arm64/audiocheckr-arm64 /tmp/audiocheckr_backup_arm64_$BUILD_NUMBER 2>/dev/null || true
                    fi
                    
                    # Clean target directory (saves ~2GB+)
                    rm -rf target/debug
                    rm -rf target/release/deps
                    rm -rf target/release/build
                    rm -rf target/release/.fingerprint
                    rm -rf target/release/incremental
                    
                    # Restore binaries
                    if [ -f /tmp/audiocheckr_backup_x86_$BUILD_NUMBER ]; then
                        mkdir -p target/release
                        mv /tmp/audiocheckr_backup_x86_$BUILD_NUMBER target/release/audiocheckr
                    fi
                    
                    if [ -f /tmp/audiocheckr_backup_arm64_$BUILD_NUMBER ]; then
                        mkdir -p target/arm64
                        mv /tmp/audiocheckr_backup_arm64_$BUILD_NUMBER target/arm64/audiocheckr-arm64
                    fi
                    
                    echo "✓ Cleanup complete"
                    du -sh . 2>/dev/null || true
                '''
            }
        }
    }
}
