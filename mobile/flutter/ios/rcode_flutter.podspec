#
# To learn more about a Podspec see http://guides.cocoapods.org/syntax/podspec.html.
# Run `pod lib lint rcode_flutter.podspec` to validate before publishing.
#
Pod::Spec.new do |s|
  s.name             = 'rcode_flutter'
  s.version          = '0.0.1'
  s.summary          = 'Flutter camera scanner for RCode.'
  s.description      = <<-DESC
Native iOS camera preview backed by the RCodeApple scanner and Rust core.
                       DESC
  s.homepage         = 'https://github.com/rcode/rcode'
  s.license          = { :type => 'MIT' }
  s.author           = { 'RCode' => 'maintainers@example.com' }
  s.source           = { :path => '.' }
  s.source_files = 'Classes/**/*'
  s.dependency 'Flutter'
  s.dependency 'RCodeApple'
  s.platform = :ios, '15.0'

  # Flutter.framework does not contain a i386 slice.
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'x86_64',
  }
  s.swift_version = '5.0'

  # If your plugin requires a privacy manifest, for example if it uses any
  # required reason APIs, update the PrivacyInfo.xcprivacy file to describe your
  # plugin's privacy impact, and then uncomment this line. For more information,
  # see https://developer.apple.com/documentation/bundleresources/privacy_manifest_files
  # s.resource_bundles = {'rcode_flutter_privacy' => ['Resources/PrivacyInfo.xcprivacy']}
end
