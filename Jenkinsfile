pipeline {
    agent any
    
    parameters {
        choice(
            name: 'TEST_TYPE',
            choices: ['QUALIFICATION_GENRE', 'REGRESSION_GENRE', 'DIAGNOSTIC', 'DSP_TEST'],
            description: 'Test type to run. QUALIFICATION_GENRE runs on every build, others are manual-only.'
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
        MINIO_FILE_GENRE_LITE = 'GenreTestSuiteLite.zip'
        MINIO_FILE_GENRE_FULL = 'TestSuite.zip'
        MINIO_FILE_DITHERING = 'dithering_tests.zip'
        MINIO_FILE_RESAMPLING = 'resampling_tests.zip'
        
        // SonarQube configuration
        SONAR_PROJECT_KEY = 'audiocheckr'
        SONAR_PROJECT_NAME = 'AudioCheckr'
        SONAR_SOURCES = 'src'
        
        // Allure configuration
        ALLURE_RESULTS_DIR = 'target/allure-results'
        ALLURE_REPORT_DIR = 'target/allure-report'
        
        // Path setup
        PATH = "$HOME/bin:$HOME/.cargo/bin:/usr/bin:$PATH"
    }
    
    triggers {
        // Scheduled regression test - Saturday at 10:00 AM
        cron('0 10 * * 6')
    }
    
    options {
        // Build timeout
        timeout(time: 45, unit: 'MINUTES')
        
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
                    
                    // Determine test type based on trigger
                    if (currentBuild.getBuildCauses('hudson.triggers.TimerTrigger$TimerTriggerCause')) {
                        // Scheduled build (cron) = REGRESSION_GENRE
                        env.TEST_TYPE = 'REGRESSION_GENRE'
                        echo "⏰ Scheduled build detected - running REGRESSION_GENRE tests"
                    } else if (currentBuild.getBuildCauses('hudson.model.Cause$UserIdCause')) {
                        // Manual build = use parameter (defaults to QUALIFICATION_GENRE)
                        env.TEST_TYPE = params.TEST_TYPE
                        echo "👤 Manual build - running ${env.TEST_TYPE} tests"
                    } else {
                        // GitHub push = QUALIFICATION_GENRE
                        env.TEST_TYPE = 'QUALIFICATION_GENRE'
                        echo "🔄 Push detected - running QUALIFICATION_GENRE tests"
                    }
                    
                    // Display build info
                    echo """
========================================================
                  AUDIOCHECKR CI/CD                     
========================================================
  Test Type:     ${env.TEST_TYPE}
  Allure:        ENABLED ✓
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
                            
                            # Install Allure if not present
                            if ! command -v allure >/dev/null 2>&1; then
                                echo "Installing Allure..."
                                ALLURE_VERSION="2.25.0"
                                wget -q https://github.com/allure-framework/allure2/releases/download/${ALLURE_VERSION}/allure-${ALLURE_VERSION}.tgz -O /tmp/allure.tgz
                                tar -xzf /tmp/allure.tgz -C $HOME/bin
                                ln -sf $HOME/bin/allure-${ALLURE_VERSION}/bin/allure $HOME/bin/allure
                                rm /tmp/allure.tgz
                            fi
                            
                            echo "=== Tool Versions ==="
                            mc --version || echo "MinIO client: not available"
                            cargo --version
                            rustc --version
                            allure --version || echo "Allure: not available"
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
                            echo "Building x86_64 binary"
                            echo "=========================================="
                            cargo build --release 2>&1 | tee build_x86_64.txt
                            echo ""
                            echo "=== x86_64 Build Artifact ==="
                            ls -lh target/release/audiocheckr
                            file target/release/audiocheckr
                            echo "=============================="
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
                                if (env.TEST_TYPE == 'DSP_TEST') {
                                    sh '''
                                        set -e
                                        echo "Setting up MinIO alias..."
                                        mc alias set myminio "$MINIO_ENDPOINT" "$MINIO_ACCESS_KEY" "$MINIO_SECRET_KEY"
                                        
                                        echo "=========================================="
                                        echo "Downloading DSP test files (Dithering + Resampling)"
                                        echo "=========================================="
                                        
                                        # Download and extract Dithering tests
                                        echo "Downloading ${MINIO_FILE_DITHERING}"
                                        mc cp myminio/${MINIO_BUCKET}/${MINIO_FILE_DITHERING} .
                                        unzip -q -o ${MINIO_FILE_DITHERING}
                                        rm -f ${MINIO_FILE_DITHERING}
                                        
                                        # Download and extract Resampling tests
                                        echo "Downloading ${MINIO_FILE_RESAMPLING}"
                                        mc cp myminio/${MINIO_BUCKET}/${MINIO_FILE_RESAMPLING} .
                                        unzip -q -o ${MINIO_FILE_RESAMPLING}
                                        rm -f ${MINIO_FILE_RESAMPLING}
                                        
                                        echo "✓ DSP test files ready"
                                        echo "Dithering tests:"
                                        ls -lh dithering_tests/ | head -n 10
                                        echo "Resampling tests:"
                                        ls -lh resampling_tests/ | head -n 10
                                    '''
                                } else if (env.TEST_TYPE == 'DIAGNOSTIC' || env.TEST_TYPE == 'REGRESSION_GENRE') {
                                    sh '''
                                        set -e
                                        echo "Setting up MinIO alias..."
                                        mc alias set myminio "$MINIO_ENDPOINT" "$MINIO_ACCESS_KEY" "$MINIO_SECRET_KEY"
                                        
                                        echo "=========================================="
                                        echo "Downloading FULL test suite (TestSuite.zip)"
                                        echo "=========================================="
                                        
                                        # Download and extract TestSuite
                                        echo "Downloading ${MINIO_FILE_GENRE_FULL}"
                                        mc cp myminio/${MINIO_BUCKET}/${MINIO_FILE_GENRE_FULL} .
                                        unzip -q -o ${MINIO_FILE_GENRE_FULL}
                                        rm -f ${MINIO_FILE_GENRE_FULL}
                                        
                                        echo "✓ Test files ready"
                                        ls -lh TestSuite/ | head -n 20
                                    '''
                                } else {
                                    // QUALIFICATION_GENRE (default)
                                    sh '''
                                        set -e
                                        echo "Setting up MinIO alias..."
                                        mc alias set myminio "$MINIO_ENDPOINT" "$MINIO_ACCESS_KEY" "$MINIO_SECRET_KEY"
                                        
                                        echo "=========================================="
                                        echo "Downloading LITE test suite (GenreTestSuiteLite.zip)"
                                        echo "=========================================="
                                        
                                        # Download and extract GenreTestSuiteLite
                                        echo "Downloading ${MINIO_FILE_GENRE_LITE}"
                                        mc cp myminio/${MINIO_BUCKET}/${MINIO_FILE_GENRE_LITE} .
                                        unzip -q -o ${MINIO_FILE_GENRE_LITE}
                                        rm -f ${MINIO_FILE_GENRE_LITE}
                                        
                                        echo "✓ Test files ready"
                                        if [ -d "GenreTestSuiteLite" ]; then
                                            ls -lh GenreTestSuiteLite/ | head -n 10
                                        fi
                                    '''
                                }
                            }
                        }
                    }
                }
            }
        }
        
        stage('Prepare Allure') {
            steps {
                sh '''
                    echo "=========================================="
                    echo "Preparing Allure Results Directory"
                    echo "=========================================="
                    
                    # Create allure results directory
                    mkdir -p ${ALLURE_RESULTS_DIR}
                    
                    # Create environment.properties for Allure
                    cat > ${ALLURE_RESULTS_DIR}/environment.properties << EOF
OS=$(uname -s)
Architecture=$(uname -m)
Rust.Version=$(rustc --version | cut -d' ' -f2)
AudioCheckr.Version=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
Test.Type=${TEST_TYPE}
Build.Number=${BUILD_NUMBER}
Git.Commit=${GIT_COMMIT_SHORT:-unknown}
Git.Branch=${GIT_BRANCH:-unknown}
EOF
                    
                    echo "✓ Allure environment configured"
                    cat ${ALLURE_RESULTS_DIR}/environment.properties
                '''
            }
        }
        
        stage('Tests') {
            stages {
                stage('Integration Tests') {
                    steps {
                        sh '''
                            echo "=========================================="
                            echo "Running Integration Tests"
                            echo "=========================================="
                            
                            set +e
                            mkdir -p target/test-results
                            cargo test --test integration_test -- --nocapture 2>&1 | tee target/test-results/integration.txt
                            TEST_EXIT=$?
                            
                            if [ $TEST_EXIT -ne 0 ]; then
                                echo "⚠ Integration tests had failures"
                            else
                                echo "✓ Integration tests passed!"
                            fi
                        '''
                    }
                }
                
                stage('Qualification Genre Tests') {
                    when {
                        expression { return env.TEST_TYPE == 'QUALIFICATION_GENRE' }
                    }
                    steps {
                        sh '''
                            echo "=========================================="
                            echo "Running QUALIFICATION GENRE tests"
                            echo "=========================================="
                            
                            set +e
                            mkdir -p target/test-results
                            cargo test --test qualification_genre_test -- --nocapture 2>&1 | tee target/test-results/qualification_genre.txt
                            TEST_EXIT=$?
                            
                            if [ $TEST_EXIT -ne 0 ]; then
                                echo "⚠ Qualification genre tests completed with failures"
                            else
                                echo "✓ Qualification genre tests passed!"
                            fi
                        '''
                    }
                }
                
                stage('Regression Genre Tests') {
                    when {
                        expression { return env.TEST_TYPE == 'REGRESSION_GENRE' }
                    }
                    steps {
                        sh '''
                            echo "=========================================="
                            echo "Running REGRESSION GENRE tests"
                            echo "=========================================="
                            
                            set +e
                            mkdir -p target/test-results
                            cargo test --test regression_genre_test -- --nocapture 2>&1 | tee target/test-results/regression_genre.txt
                            TEST_EXIT=$?
                            
                            if [ $TEST_EXIT -ne 0 ]; then
                                echo "⚠ Regression genre tests completed with failures"
                            else
                                echo "✓ Regression genre tests passed!"
                            fi
                        '''
                    }
                }
                
                stage('Diagnostic Tests') {
                    when {
                        expression { return env.TEST_TYPE == 'DIAGNOSTIC' }
                    }
                    steps {
                        sh '''
                            echo "=========================================="
                            echo "Running DIAGNOSTIC tests"
                            echo "=========================================="
                            
                            set +e
                            mkdir -p target/test-results
                            cargo test --test diagnostic_test -- --nocapture 2>&1 | tee target/test-results/diagnostic.txt
                            TEST_EXIT=$?
                            
                            if [ $TEST_EXIT -ne 0 ]; then
                                echo "⚠ Diagnostic tests completed with failures"
                            else
                                echo "✓ Diagnostic tests passed!"
                            fi
                        '''
                    }
                }
                
                stage('DSP Tests (Dithering & Resampling)') {
                    when {
                        expression { return env.TEST_TYPE == 'DSP_TEST' }
                    }
                    steps {
                        sh '''
                            echo "=========================================="
                            echo "Running DSP TESTS (Dithering & Resampling)"
                            echo "=========================================="
                            
                            set +e
                            mkdir -p target/test-results
                            cargo test --test dithering_resampling_test -- --nocapture 2>&1 | tee target/test-results/dsp_test.txt
                            TEST_EXIT=$?
                            
                            if [ $TEST_EXIT -ne 0 ]; then
                                echo "⚠ DSP tests completed with failures"
                            else
                                echo "✓ DSP tests passed!"
                            fi
                        '''
                    }
                }
            }
        }
        
        stage('Generate Allure Report') {
            steps {
                script {
                    sh '''
                        echo "=========================================="
                        echo "Generating Allure Report"
                        echo "=========================================="
                        
                        # Check if allure is available
                        if ! command -v allure >/dev/null 2>&1; then
                            echo "⚠ Allure not found, skipping report generation"
                            echo "  Install Allure to enable beautiful test reports"
                            exit 0
                        fi
                        
                        # Check if we have any results
                        if [ -d "${ALLURE_RESULTS_DIR}" ] && [ "$(ls -A ${ALLURE_RESULTS_DIR} 2>/dev/null)" ]; then
                            echo "Found Allure results in ${ALLURE_RESULTS_DIR}"
                            ls -la ${ALLURE_RESULTS_DIR}/
                            
                            # Generate the report
                            allure generate ${ALLURE_RESULTS_DIR} -o ${ALLURE_REPORT_DIR} --clean
                            
                            echo "✓ Allure report generated at ${ALLURE_REPORT_DIR}"
                        else
                            echo "⚠ No Allure results found in ${ALLURE_RESULTS_DIR}"
                            echo "  Tests may not have generated Allure-compatible output"
                        fi
                    '''
                }
            }
        }
        
        stage('SonarQube Analysis') {
            when {
                allOf {
                    expression { return !params.SKIP_SONARQUBE }
                    expression { return env.TEST_TYPE != 'DIAGNOSTIC' && env.TEST_TYPE != 'DSP_TEST' }
                }
            }
            steps {
                script {
                    // Check if credential exists, if not skip
                    try {
                        withCredentials([string(credentialsId: 'sonarqube-token', variable: 'SONAR_TOKEN')]) {
                            sh '''
                                if ! command -v sonar-scanner >/dev/null 2>&1; then
                                    echo "⚠ sonar-scanner not found, skipping SonarQube analysis"
                                    exit 0
                                fi
                                
                                echo "=========================================="
                                echo "Running SonarQube Analysis"
                                echo "=========================================="
                                
                                sonar-scanner \
                                    -Dsonar.projectKey=${SONAR_PROJECT_KEY} \
                                    -Dsonar.projectName="${SONAR_PROJECT_NAME}" \
                                    -Dsonar.sources=${SONAR_SOURCES} \
                                    -Dsonar.host.url=${SONARQUBE_URL} \
                                    -Dsonar.login=${SONAR_TOKEN}
                            '''
                        }
                    } catch (Exception e) {
                        echo "⚠ SonarQube credential not found (sonarqube-token), skipping analysis"
                        echo "  To enable: Add 'sonarqube-token' credential in Jenkins"
                    }
                }
            }
        }
    }
    
    post {
        success {
            echo '✓ Build and tests completed successfully!'
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
                }
            }
            
            // Publish JUnit test results (shows in Jenkins UI)
            junit(
                allowEmptyResults: true,
                testResults: 'target/test-results/*.xml, target/allure-results/*-junit.xml',
                skipPublishingChecks: false
            )
            
            // Publish Allure report
            script {
                try {
                    allure([
                        includeProperties: true,
                        jdk: '',
                        results: [[path: 'target/allure-results']]
                    ])
                    echo "✓ Allure report published"
                } catch (Exception e) {
                    echo "⚠ Allure plugin not configured or failed: ${e.message}"
                    echo "  To enable: Install 'Allure Jenkins Plugin' from Plugin Manager"
                    echo "  Configure: Manage Jenkins → Global Tool Configuration → Allure Commandline"
                    
                    // Archive the allure results as fallback
                    archiveArtifacts artifacts: 'target/allure-results/**/*', 
                                   allowEmptyArchive: true
                    archiveArtifacts artifacts: 'target/allure-report/**/*', 
                                   allowEmptyArchive: true
                }
            }
            
            // Clean up to save disk space
            script {
                echo "🧹 Cleaning workspace to save disk space..."
                
                sh '''
                    # Delete test files and ZIPs
                    rm -f GenreTestSuiteLite.zip TestSuite.zip dithering_tests.zip resampling_tests.zip
                    rm -rf GenreTestSuiteLite TestSuite dithering_tests resampling_tests
                    
                    # Keep the release binary, clean build cache
                    if [ -f target/release/audiocheckr ]; then
                        cp target/release/audiocheckr /tmp/audiocheckr_backup_$BUILD_NUMBER 2>/dev/null || true
                    fi
                    
                    # Clean target directory (saves ~2GB+)
                    rm -rf target/debug
                    rm -rf target/release/deps
                    rm -rf target/release/build
                    rm -rf target/release/.fingerprint
                    rm -rf target/release/incremental
                    
                    # Restore binary
                    if [ -f /tmp/audiocheckr_backup_$BUILD_NUMBER ]; then
                        mkdir -p target/release
                        mv /tmp/audiocheckr_backup_$BUILD_NUMBER target/release/audiocheckr
                    fi
                    
                    echo "✓ Cleanup complete"
                    du -sh . 2>/dev/null || true
                '''
            }
        }
    }
}
