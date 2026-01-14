def sendDiscordNotification(webhookUrl, status) {
    def color
    def emoji
    def message

    switch(status) {
    case 'SUCCESS':
    color = 3066993  // Green
    emoji = '✅'
    message = 'Build completed successfully!'
    break
    case 'UNSTABLE':
    color = 16776960  // Yellow
    emoji = '⚠️'
    message = 'Build completed with test failures'
    break
    case 'FAILURE':
    color = 15158332  // Red
    emoji = '❌'
    message = 'Build failed!'
    break
    default:
    color = 9807270  // Gray
    emoji = 'ℹ️'
    message = 'Build completed'
    }

    discordSend(
        webhookURL: webhookUrl,
        title: "${emoji} AudioCheckr Build #${env.BUILD_NUMBER}",
        description: """
                        **Status:** ${message}
                        **Test Type:** ${env.TEST_TYPE}
                        **Commit:** ${env.GIT_COMMIT_SHORT ?: 'unknown'} by ${env.GIT_AUTHOR ?: 'unknown'}
                        **Message:** ${env.GIT_COMMIT_MSG ?: 'No commit message'}
                        **Duration:** ${currentBuild.durationString.replace(' and counting', '')}
        """.trim(),
        link: env.BUILD_URL,
        result: status,
        thumbnail: 'https://jenkins.io/images/logos/jenkins/jenkins.png',
        customAvatarUrl: 'https://jenkins.io/images/logos/jenkins/jenkins.png',
        customUsername: 'Jenkins AudioCheckr',
        notes: "Build on ${env.NODE_NAME}",
        successful: status == 'SUCCESS'
    )
}

def determineTestTypeFromChanges() {
    // Get list of changed files in the commit
    def changedFiles = sh(
        script: 'git diff --name-only HEAD~1 HEAD 2>/dev/null || git diff --name-only HEAD',
        returnStdout: true
    ).trim().split('\n')
    
    echo "Changed files:"
    changedFiles.each { file ->
        echo "  - ${file}"
    }
    
    // Check for MQA-related changes
    def mqaFiles = [
        'src/core/analysis/mqa_detection.rs',
        'tests/mqa_test.rs'
    ]
    if (changedFiles.any { file -> mqaFiles.contains(file) }) {
        echo "🎯 MQA-related files changed - running MQA_TEST"
        return 'MQA_TEST'
    }
    
    // Check for DSP-related changes
    def dspFiles = [
        'src/core/analysis/dither.rs',
        'src/core/analysis/dither_detection.rs',
        'src/core/analysis/resample_detection.rs',
        'src/core/analysis/upsampling.rs',
        'tests/dithering_resampling_test.rs'
    ]
    if (changedFiles.any { file -> dspFiles.contains(file) }) {
        echo "🎯 DSP-related files changed - running DSP_TEST"
        return 'DSP_TEST'
    }
    
    // Check for specific test file changes
    if (changedFiles.contains('tests/diagnostic_test.rs')) {
        echo "🎯 Diagnostic test file changed - running DIAGNOSTIC"
        return 'DIAGNOSTIC'
    }
    
    if (changedFiles.contains('tests/dsp_diagnostic_test.rs')) {
        echo "🎯 DSP diagnostic test file changed - running DSP_DIAGNOSTIC"
        return 'DSP_DIAGNOSTIC'
    }
    
    // Default to qualification genre tests for all other changes
    echo "📋 Other changes detected - running QUALIFICATION_GENRE"
    return 'QUALIFICATION_GENRE'
}

pipeline {
    agent any
    


    parameters {
        choice(
            name: 'TEST_TYPE',
            choices: ['QUALIFICATION_GENRE', 'REGRESSION_GENRE', 'DIAGNOSTIC', 'DSP_TEST', 'DSP_DIAGNOSTIC', 'MQA_TEST'],
            description: 'Test type to run. QUALIFICATION_GENRE runs on every build, others are manual-only. DSP_DIAGNOSTIC runs detailed diagnostic tests on dithering/resampling files. MQA_TEST runs MQA detection tests on Tidal MQA files.'
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
        MINIO_FILE_MQA = 'MQA.zip'

        // SonarQube configuration
        SONAR_PROJECT_KEY = 'audiocheckr'
        SONAR_PROJECT_NAME = 'AudioCheckr'
        SONAR_SOURCES = 'src'
        SONARQUBE_URL = "${env.SONARQUBE_URL ?: 'http://192.168.178.101:9000'}"

        // Allure configuration
        ALLURE_RESULTS_DIR = 'target/allure-results'
        ALLURE_REPORT_DIR = 'target/allure-report'

        // Path setup
        PATH = "$HOME/bin:$HOME/.cargo/bin:/usr/bin:$PATH"

        // CI marker for test awareness
        CI = 'true'
    }

    triggers {
        // Scheduled regression test - Saturday at 10:00 AM
        cron('0 10 * * 6')
    }

    options {
        // Build timeout - increased for DSP tests with large files
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

                    // Determine test type based on trigger and file changes
                    if (currentBuild.getBuildCauses('hudson.triggers.TimerTrigger$TimerTriggerCause')) {
                        // Scheduled build (cron) = REGRESSION_GENRE
                        env.TEST_TYPE = 'REGRESSION_GENRE'
                        echo "⏰ Scheduled build detected - running REGRESSION_GENRE tests"
                    } else if (currentBuild.getBuildCauses('hudson.model.Cause$UserIdCause')) {
                        // Manual build = use parameter (defaults to QUALIFICATION_GENRE)
                        env.TEST_TYPE = params.TEST_TYPE
                        echo "👤 Manual build - running ${env.TEST_TYPE} tests"
                    } else {
                        // GitHub push = intelligent test selection based on changed files
                        env.TEST_TYPE = determineTestTypeFromChanges()
                        echo "🔄 Push detected - intelligently selected ${env.TEST_TYPE} tests based on file changes"
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
                            echo "Building x86_64 binary (RELEASE mode)"
                            echo "=========================================="
                            # Always build release for faster audio processing
                            cargo build --release 2>&1 | tee build_x86_64.txt
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
                                if (env.TEST_TYPE == 'DSP_TEST' || env.TEST_TYPE == 'DSP_DIAGNOSTIC') {
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
                                } else if (env.TEST_TYPE == 'MQA_TEST') {
                                    sh '''
                                        set -e
                                        echo "Setting up MinIO alias..."
                                        mc alias set myminio "$MINIO_ENDPOINT" "$MINIO_ACCESS_KEY" "$MINIO_SECRET_KEY"
                                        
                                        echo "=========================================="
                                        echo "Downloading MQA test files"
                                        echo "=========================================="
                                        
                                        # Download and extract MQA tests
                                        echo "Downloading ${MINIO_FILE_MQA}"
                                        mc cp myminio/${MINIO_BUCKET}/${MINIO_FILE_MQA} .
                                        unzip -q -o ${MINIO_FILE_MQA}
                                        rm -f ${MINIO_FILE_MQA}
                                        
                                        echo "✓ MQA test files ready"
                                        echo "MQA test files:"
                                        ls -lh MQA/ | head -n 20
                                        echo "Total files:"
                                        find MQA -name "*.flac" | wc -l
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
CI.Environment=Jenkins
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
                            cargo test --test integration_test --release -- --nocapture 2>&1 | tee target/test-results/integration.txt
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
                            cargo test --test qualification_genre_test --release -- --nocapture 2>&1 | tee target/test-results/qualification_genre.txt
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
                            cargo test --test regression_genre_test --release -- --nocapture 2>&1 | tee target/test-results/regression_genre.txt
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
                            cargo test --test diagnostic_test --release -- --nocapture 2>&1 | tee target/test-results/diagnostic.txt
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
                            echo "NOTE: Using RELEASE build for faster audio processing"
                            echo "NOTE: Reduced parallelism (1 thread) for CI stability"
                            echo "=========================================="
                            
                            set +e
                            mkdir -p target/test-results
                            
                            # Use --release for MUCH faster audio processing
                            cargo test --test dithering_resampling_test --release -- --nocapture --test-threads=1 2>&1 | tee target/test-results/dsp_test.txt
                            TEST_EXIT=$?
                            
                            if [ $TEST_EXIT -ne 0 ]; then
                                echo "⚠ DSP tests completed with failures"
                            else
                                echo "✓ DSP tests passed!"
                            fi
                        '''
                    }
                }

                stage('DSP Diagnostic Tests') {
                    when {
                        expression { return env.TEST_TYPE == 'DSP_DIAGNOSTIC' }
                    }
                    steps {
                        sh '''
                            echo "=========================================="
                            echo "Running DSP DIAGNOSTIC tests"
                            echo "=========================================="
                            echo "This provides detailed diagnostic output"
                            echo "showing what each detector sees on DSP files"
                            echo "=========================================="
                            
                            set +e
                            mkdir -p target/test-results
                            mkdir -p target/dsp-diagnostics
                            
                            # Run the diagnostic tests with verbose output
                            cargo test --test dsp_diagnostic_test --release -- --nocapture 2>&1 | tee target/test-results/dsp_diagnostic.txt
                            TEST_EXIT=$?
                            
                            echo ""
                            echo "=========================================="
                            echo "DSP Diagnostic Summary"
                            echo "=========================================="
                            
                            if [ $TEST_EXIT -ne 0 ]; then
                                echo "⚠ DSP diagnostic tests completed with some failures"
                                echo "Check output above for false positive analysis"
                            else
                                echo "✓ DSP diagnostic tests passed!"
                            fi
                            
                            # Archive diagnostic reports if generated
                            if [ -d "target/dsp-diagnostics" ] && [ "$(ls -A target/dsp-diagnostics 2>/dev/null)" ]; then
                                echo "Diagnostic reports available in target/dsp-diagnostics/"
                                ls -la target/dsp-diagnostics/
                            fi
                        '''
                    }
                }

                stage('MQA Detection Tests') {
                    when {
                        expression { return env.TEST_TYPE == 'MQA_TEST' }
                    }
                    steps {
                        sh '''
                            echo "=========================================="
                            echo "Running MQA DETECTION tests"
                            echo "=========================================="
                            echo "Testing MQA detection on Tidal MQA files"
                            echo "=========================================="
                            
                            set +e
                            mkdir -p target/test-results
                            
                            # Verify MQA folder exists
                            if [ ! -d "MQA" ]; then
                                echo "ERROR: MQA folder not found!"
                                exit 1
                            fi
                            
                            echo "MQA folder contents:"
                            ls -la MQA/
                            echo ""
                            echo "FLAC file count: $(find MQA -name '*.flac' | wc -l)"
                            echo ""
                            
                            # Run the MQA tests (--ignored to run the ignored tests)
                            cargo test --test mqa_test --release -- --ignored --nocapture --test-threads=1 2>&1 | tee target/test-results/mqa_test.txt
                            TEST_EXIT=$?
                            
                            echo ""
                            echo "=========================================="
                            echo "MQA Detection Summary"
                            echo "=========================================="
                            
                            if [ $TEST_EXIT -ne 0 ]; then
                                echo "⚠ MQA tests completed with some failures"
                                echo "Check output above for detection accuracy"
                            else
                                echo "✓ MQA tests passed!"
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
                    expression { return env.TEST_TYPE != 'DIAGNOSTIC' && env.TEST_TYPE != 'DSP_TEST' && env.TEST_TYPE != 'DSP_DIAGNOSTIC' && env.TEST_TYPE != 'MQA_TEST' }
                }
            }
            steps {
                script {
                    try {
                        def scannerHome = tool name: 'SonarQube Scanner', type: 'hudson.plugins.sonar.SonarRunnerInstallation'
                        
                        withCredentials([string(credentialsId: 'sonarqube-token', variable: 'SONAR_TOKEN')]) {
                            echo "=========================================="
                            echo "Running SonarQube Analysis"
                            echo "=========================================="
                            
                            sh """
                                \"${scannerHome}/bin/sonar-scanner\" \\
                                    -Dsonar.projectKey=${SONAR_PROJECT_KEY} \\
                                    -Dsonar.projectName=\"${SONAR_PROJECT_NAME}\" \\
                                    -Dsonar.sources=${SONAR_SOURCES} \\
                                    -Dsonar.host.url=${SONARQUBE_URL} \\
                                    -Dsonar.token=\$SONAR_TOKEN
                            """
                        }
                    } catch (Exception e) {
                        echo "⚠ SonarQube analysis failed: ${e.message}"
                        echo "  Verify: 'sonarqube-token' credential is valid"
                    }
                }
            }
        }
    }

    post {
        success {
            echo '✓ Build and tests completed successfully!'
            script {
                withCredentials([string(credentialsId: 'audiocheckrDiscordWebhook', variable: 'DISCORD_WEBHOOK')]) {
                    sendDiscordNotification(DISCORD_WEBHOOK, 'SUCCESS')
                }
            }
        }
        unstable {
            echo '⚠ Build completed but some tests failed. Check test results.'
            script {
                withCredentials([string(credentialsId: 'audiocheckrDiscordWebhook', variable: 'DISCORD_WEBHOOK')]) {
                    sendDiscordNotification(DISCORD_WEBHOOK, 'UNSTABLE')
                }
            }
        }
        failure {
            echo '✗ Build or tests failed. Check logs for details.'
            script {
                withCredentials([string(credentialsId: 'audiocheckrDiscordWebhook', variable: 'DISCORD_WEBHOOK')]) {
                    sendDiscordNotification(DISCORD_WEBHOOK, 'FAILURE')
                }
            }
        }
        always {
            script {
                // Archive x86_64 binary (on success or unstable)
                if (currentBuild.result != 'FAILURE') {
                    archiveArtifacts artifacts: 'target/release/audiocheckr',
                    fingerprint: true,
                    allowEmptyArchive: true
                }

                // Archive DSP diagnostic reports
                if (env.TEST_TYPE == 'DSP_DIAGNOSTIC') {
                    archiveArtifacts artifacts: 'target/dsp-diagnostics/**/*',
                    allowEmptyArchive: true
                    archiveArtifacts artifacts: 'target/test-results/dsp_diagnostic.txt',
                    allowEmptyArchive: true
                }

                // Archive MQA test results
                if (env.TEST_TYPE == 'MQA_TEST') {
                    archiveArtifacts artifacts: 'target/test-results/mqa_test.txt',
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
                rm -f GenreTestSuiteLite.zip TestSuite.zip dithering_tests.zip resampling_tests.zip MQA.zip
                rm -rf GenreTestSuiteLite TestSuite dithering_tests resampling_tests MQA

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