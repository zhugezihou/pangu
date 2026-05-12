from setuptools import setup, find_packages

setup(
    name="pangu-cli",
    version="0.1.0",
    packages=find_packages(),
    scripts=["pangu_cli.py"],
    python_requires=">=3.8",
)
